//! OpenTelemetry (OTLP / http-protobuf) tracing bootstrap.
//!
//! Traces are exported to the Elastic APM Server's native OTLP endpoint
//! (`<ELASTIC_APM_SERVER_URL>/v1/traces`), which apm-server registers unconditionally on
//! the same port as the classic intake and behind the same secret-token auth.

use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _; // `provider.tracer(..)` is a TRAIT method.
use opentelemetry::KeyValue;
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

fn env_clean(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Owns the `SdkTracerProvider` so the last batch can be flushed on shutdown.
///
/// `global::set_tracer_provider` keeps a clone of the provider FOREVER, so `Drop` never
/// runs on the global copy and an explicit `shutdown()` is mandatory -- without it the
/// in-flight batch is silently lost on every rolling deploy.
pub struct ApmGuard {
    provider: SdkTracerProvider,
}

impl ApmGuard {
    /// Flush + stop the exporter. `shutdown()` already force-flushes, so there is no
    /// separate `force_flush()` call and no `thread::sleep` drain.
    pub fn shutdown(self) {
        // The drain deadline MUST exceed the exporter's own `.with_timeout(10s)`. The
        // parameterless `shutdown()` hard-codes a 5s deadline, which is the `recv_timeout`
        // on the shutdown ack: if the final flush POST takes >5s but <10s (cold TLS
        // handshake, retried connect), that timer fires first, `handle.join()` is skipped,
        // and the process exits with the export still in flight -- the last batch is
        // dropped. Using 12s (> 10s export timeout) lets a slow final export complete.
        match self.provider.shutdown_with_timeout(Duration::from_secs(12)) {
            Ok(()) => println!("✅ APM tracer provider flushed and shut down."),
            // A repeat call returns AlreadyShutdown; that is benign, not an error.
            Err(e) => println!("⚠️ APM tracer provider shutdown: {}", e),
        }
    }
}

/// Build `<base>/v1/traces`, idempotently.
///
/// A scheme is MANDATORY: `http::Uri` happily parses a bare `apm.example.com:8200` as a
/// PATH (with no authority), and the exporter then fails at request time with an opaque
/// error instead of at boot.
fn build_traces_endpoint(base: &str) -> Result<String, Box<dyn std::error::Error>> {
    let base = base.trim().trim_end_matches('/');
    let lower = base.to_ascii_lowercase();

    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(format!(
            "ELASTIC_APM_SERVER_URL must start with http:// or https:// (got {:?}); \
             a bare host is parsed as a URI path and silently breaks the exporter",
            base
        )
        .into());
    }

    if lower.ends_with("/v1/traces") {
        Ok(base.to_string())
    } else {
        Ok(format!("{}/v1/traces", base))
    }
}

/// Sample ratio, clamped to 0.0..=1.0. `OTEL_TRACES_SAMPLER_ARG` wins over the Elastic name.
fn sample_ratio() -> f64 {
    env_clean("OTEL_TRACES_SAMPLER_ARG")
        .or_else(|| env_clean("ELASTIC_APM_TRANSACTION_SAMPLE_RATE"))
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(1.0)
}

/// Returns `Ok(Some(guard))` when the tracing subscriber was installed; `Ok(None)` when APM
/// is disabled by configuration or a subscriber was already present.
pub fn init_apm() -> Result<Option<ApmGuard>, Box<dyn std::error::Error>> {
    // ---- HYBRID CONFIG -----------------------------------------------------------
    // The ELASTIC_APM_* vars remain the supported way to configure this service; the
    // standard OTEL_* vars, when present, WIN. That keeps existing deployments working
    // untouched while letting an operator point the service at a collector without a
    // rebuild.
    //
    // Endpoint precedence is enforced by NOT calling `.with_endpoint()` at all when an
    // OTEL endpoint var is set: a programmatic `.with_endpoint()` overrides the env vars
    // inside opentelemetry-otlp (resolve_http_endpoint), so the only way to let the env
    // win is to stay silent.
    let otel_endpoint_from_env = env_clean("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
        .or_else(|| env_clean("OTEL_EXPORTER_OTLP_ENDPOINT"));

    let apm_server_url = env_clean("ELASTIC_APM_SERVER_URL");
    let service_name = env_clean("OTEL_SERVICE_NAME").or_else(|| env_clean("ELASTIC_APM_SERVICE_NAME"));

    if otel_endpoint_from_env.is_none() && apm_server_url.is_none() {
        println!("⚠️ Variabel APM (URL atau SERVICE_NAME) not found. Skipping init APM.");
        return Ok(None);
    }
    let service_name = match service_name {
        Some(name) => name,
        None => {
            println!("⚠️ Variabel APM (URL atau SERVICE_NAME) not found. Skipping init APM.");
            return Ok(None);
        }
    };

    // ---- exporter ----------------------------------------------------------------
    let mut builder = SpanExporter::builder()
        .with_http()
        // Cosmetic/self-documenting: the wire format actually comes from the `http-proto`
        // cargo feature. It must NOT be Protocol::Grpc here (hard build error).
        .with_protocol(Protocol::HttpBinary)
        .with_timeout(Duration::from_secs(10));

    let resolved_endpoint = match &otel_endpoint_from_env {
        Some(url) => {
            // Note the differing SDK semantics we are deliberately not second-guessing:
            // OTEL_EXPORTER_OTLP_TRACES_ENDPOINT is used verbatim, while
            // OTEL_EXPORTER_OTLP_ENDPOINT gets "/v1/traces" appended by the SDK.
            format!("{} (from OTEL_EXPORTER_OTLP_*_ENDPOINT, resolved by the SDK)", url)
        }
        None => {
            let endpoint = build_traces_endpoint(apm_server_url.as_deref().unwrap())?;
            builder = builder.with_endpoint(endpoint.clone());
            endpoint
        }
    };

    let mut headers: HashMap<String, String> = HashMap::new();
    match env_clean("ELASTIC_APM_SECRET_TOKEN") {
        Some(token) => {
            // apm-server guards /v1/traces with the same secret token as the classic intake.
            headers.insert("Authorization".to_string(), format!("Bearer {}", token));
            println!("🔒 Using Secret Token.");
        }
        None => println!("⚠️ No Authorization Found for APM."),
    }
    if !headers.is_empty() {
        builder = builder.with_headers(headers);
    }

    let exporter = builder.build()?;

    // ---- batch processor ---------------------------------------------------------
    // In opentelemetry_sdk 0.32 this processor owns a DEDICATED OS THREAD (no
    // `runtime::Tokio` parameter any more), which is exactly what we need under
    // actix_rt's current-thread runtime.
    //
    // max_export_batch_size is kept modest on purpose: one export is one HTTP POST whose
    // body grows with the batch, and our spans are attribute-heavy (251 instrumented
    // sites, ~184 of them DB spans per request). 128 keeps the body well under the
    // typical nginx `client_max_body_size 1m` in front of apm-server.
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(4096)
        .with_max_export_batch_size(128)
        .with_scheduled_delay(Duration::from_secs(2))
        // NOTE: `BatchConfigBuilder::with_max_export_timeout` exists in opentelemetry_sdk
        // 0.32 only behind the `experimental_trace_batch_span_processor_with_async_runtime`
        // feature -- the thread-based processor we use does not consult it at all. The
        // export deadline is enforced by the exporter's own `.with_timeout(10s)` above.
        .build();

    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(batch_config)
        .build();

    // ---- resource ----------------------------------------------------------------
    // `Resource::builder()` (NOT `builder_empty()`) pre-loads the SDK, telemetry and ENV
    // detectors, so OTEL_SERVICE_NAME / OTEL_RESOURCE_ATTRIBUTES keep working for
    // operators and telemetry.sdk.* (-> agent.name/version in Elastic) is populated.
    let environment = env_clean("ELASTIC_APM_ENVIRONMENT").unwrap_or_else(|| "development".to_string());
    let hostname = env_clean("HOSTNAME");

    let mut attributes = vec![
        KeyValue::new("service.version", std::env!("CARGO_PKG_VERSION")),
        // Emit BOTH spellings with identical values: apm-server only maps
        // `deployment.environment.name` from 8.16 onward, while `deployment.environment`
        // is understood by every version. Both land on `service.environment`.
        //
        // These are written as plain string literals on purpose: in
        // opentelemetry-semantic-conventions 0.32 the legacy consts are gated behind the
        // `semconv_experimental` feature AND #[deprecated], so using them costs a feature
        // flag and a warning for zero benefit.
        KeyValue::new("deployment.environment", environment.clone()),
        KeyValue::new("deployment.environment.name", environment.clone()),
        KeyValue::new("host.arch", std::env::consts::ARCH),
        KeyValue::new("os.type", std::env::consts::OS),
    ];

    if let Some(host) = &hostname {
        attributes.push(KeyValue::new("host.name", host.clone()));
        // service.instance.id distinguishes replicas of the same service in the UI.
        attributes.push(KeyValue::new("service.instance.id", host.clone()));
    }

    // Ported from the old detect_container(): container.id (was `system.container.id`).
    if std::env::var("CONTAINER_NAME").is_ok() || std::path::Path::new("/.dockerenv").exists() {
        match &hostname {
            Some(host) => attributes.push(KeyValue::new("container.id", host.clone())),
            // The old code invented a random UUID here. A per-boot random container id is
            // worse than none: it is an unbounded GLOBAL label on every event of the
            // service (unmatched resource attributes become global labels in apm-data).
            None => eprintln!("⚠️: HOSTNAME not found. Skipping container.id."),
        }
    }

    // Ported from the old detect_kubernetes().
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        if let Some(ns) = env_clean("KUBE_NAMESPACE") {
            attributes.push(KeyValue::new("k8s.namespace.name", ns));
        }
        if let Some(pod) = &hostname {
            attributes.push(KeyValue::new("k8s.pod.name", pod.clone()));
        }
        if let Some(uid) = env_clean("KUBE_POD_UID") {
            attributes.push(KeyValue::new("k8s.pod.uid", uid));
        }
        if let Some(node) = env_clean("KUBE_NODE_NAME") {
            attributes.push(KeyValue::new("k8s.node.name", node));
        }
    }

    // No schema_url: opentelemetry-semantic-conventions advertises 1.42 while apm-data
    // pins 1.27 and we deliberately emit 1.27-era attribute keys.
    let resource = Resource::builder()
        .with_service_name(service_name.clone())
        .with_attributes(attributes)
        .build();

    // ---- sampler -----------------------------------------------------------------
    // ParentBased so an inbound sampled traceparent is always honoured end-to-end; the
    // ratio only applies to locally-started (root) traces.
    let ratio = sample_ratio();
    let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio)));

    let provider = SdkTracerProvider::builder()
        .with_span_processor(processor)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("csml_server");

    // The provider is cheap to clone (Arc inside) and both clones point at the same
    // processor, so the guard's shutdown() drains the batch the global copy fed.
    global::set_tracer_provider(provider.clone());

    // MANDATORY: the default global propagator is a NOOP one and OTEL_PROPAGATORS is not
    // read by the SDK at all. tracing-actix-web's set_otel_parent() extracts the inbound
    // W3C `traceparent` through the GLOBAL propagator, so without this line every
    // incoming request silently starts a brand-new trace.
    global::set_text_map_propagator(TraceContextPropagator::new());

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // 3. Inisialisasi Global Subscriber
    //
    // WARNING: EnvFilter::from_default_env() installs an ERROR-ONLY filter when RUST_LOG
    // is unset or empty. #[instrument] spans are INFO, so that silently disables every
    // span callsite in the process and NOTHING is ever sent to APM. Always fall back to
    // an explicit usable filter.
    //
    // WARNING: this EnvFilter MUST stay a global registry layer. Do NOT move it onto
    // otel_layer via `.with_filter(...)`: a per-layer filter lets a child span reach the
    // exporting layer while its parent was filtered out of that layer only, which broke
    // the previous crate outright and still produces orphaned/mis-parented spans here.
    //
    // WARNING: never set a bare level here or in RUST_LOG (e.g. RUST_LOG=debug). That
    // enables the reqwest/hyper/tokio callsites *inside* the OTLP exporter itself, whose
    // spans are then exported, which emits more requests -- a self-amplifying loop.
    // Keep csml_engine at `info`: the private high-fan-out DB helpers are deliberately
    // registered at `debug`.
    //
    // NOTE: `tracing_actix_web=info` is LOAD-BEARING. TracingLogger's root span is
    // emitted at INFO from target `tracing_actix_web`; with the base directive at `warn`
    // it would be filtered out, every request would lose its transaction, and all 23
    // handler-thread child spans would become orphans.
    const DEFAULT_FILTER: &str = "warn,csml_server=info,csml_engine=info,csml_interpreter=warn,tracing_actix_web=info,hyper=off,h2=off,reqwest=off,tokio=off,rustls=off";

    let filter_directives = match std::env::var("RUST_LOG") {
        // `tracing_actix_web=info` is APPENDED, not merely defaulted: RUST_LOG fully
        // REPLACES DEFAULT_FILTER, so without this a deployment that already sets RUST_LOG
        // would filter out TracingLogger's INFO root span (base directive `warn`) and lose
        // every transaction, orphaning all 23 handler-thread child spans. EnvFilter dedups
        // by target, so appending makes this directive win over any operator-supplied value
        // for the same target.
        Ok(v) if !v.trim().is_empty() => format!("{},tracing_actix_web=info", v.trim()),
        _ => DEFAULT_FILTER.to_string(),
    };
    println!("🔎 Tracing filter: {}", filter_directives);

    let env_filter = EnvFilter::try_new(&filter_directives)
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let result = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .try_init();

    match result {
        Ok(_) => {
            // Cap the log -> tracing bridge (installed by try_init when the tracing-log
            // feature is compiled in) at CSML_LOG_LEVEL, so enabling span tracing does
            // NOT also unmute csml_logger's Info-level `db call save messages {:?}`
            // (db_connectors/messages.rs:26) and `db call set state ... {:?}`
            // (db_connectors/state.rs:183), which print message bodies and cleartext
            // bot variables. Preserves today's behaviour exactly.
            let log_level = match std::env::var("CSML_LOG_LEVEL")
                .unwrap_or_else(|_| "error".to_string())
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "trace" => log::LevelFilter::Trace,
                "debug" => log::LevelFilter::Debug,
                "info" => log::LevelFilter::Info,
                "warn" => log::LevelFilter::Warn,
                "off" => log::LevelFilter::Off,
                _ => log::LevelFilter::Error,
            };
            log::set_max_level(log_level);

            println!("📡 OTLP traces endpoint: {}", resolved_endpoint);
            println!("🎲 Trace sample ratio: {} (parent-based)", ratio);
            println!("✅ Elastic APM Tracing Ready for sending traces.");
            Ok(Some(ApmGuard { provider }))
        }
        Err(e) => {
            println!("⚠️ Tracing init returned: {} (subscriber may already be installed)", e);
            // Nothing will ever feed this provider; tear down its exporter thread now
            // rather than leaking it for the lifetime of the process.
            let _ = provider.shutdown();
            Ok(None)
        }
    }
}
