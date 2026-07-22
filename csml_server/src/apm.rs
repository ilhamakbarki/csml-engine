use tracing_elastic_apm::config::{Config, Service, Authorization};
use tracing_elastic_apm::model::{Language, Runtime};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, filter::EnvFilter};
use tracing_elastic_apm::model::{System, Container, Kubernetes, Pod, Node, ServiceNode};

fn env_clean(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Returns Ok(true) when the tracing subscriber was installed; Ok(false) when APM
/// is disabled by configuration or a subscriber was already present.
pub fn init_apm() -> Result<bool, Box<dyn std::error::Error>> {
    let apm_server_url = env_clean("ELASTIC_APM_SERVER_URL");
    let service_name = env_clean("ELASTIC_APM_SERVICE_NAME");

    if apm_server_url.is_none() || service_name.is_none() {
        println!("⚠️ Variabel APM (URL atau SERVICE_NAME) not found. Skipping init APM.");
        return Ok(false);
    }

    let apm_server_url = apm_server_url.unwrap();
    let service_name = service_name.unwrap();
    let apm_server_url = apm_server_url.trim_end_matches('/').to_string();

    let apm_secret_token = env_clean("ELASTIC_APM_SECRET_TOKEN");
    let service_version = std::env!("CARGO_PKG_VERSION").to_owned();
    let environment = Some(env_clean("ELASTIC_APM_ENVIRONMENT").unwrap_or_else(|| "development".to_string()));
    let rust_version = std::env::var("RUST_VERSION").unwrap_or_else(|_| "N/A".to_string());

    let apm_system = System {
        hostname: std::env::var("HOSTNAME").ok(),
        architecture: Some(std::env::consts::ARCH.to_string()),
        platform: Some(std::env::consts::OS.to_string()),
        container: detect_container(),
        kubernetes: detect_kubernetes(),
        detected_hostname: None, 
        configured_hostname: None,
    };

    let apm_language = Language {
        name: "Rust".to_string(),
        version: Some(rust_version.clone()),
    };
    
    let apm_runtime = Runtime {
        name: "Rust".to_string(),
        version: rust_version,
    };

    let apm_service_node = ServiceNode {
        configured_name: std::env::var("HOSTNAME").ok()
    };

    let apm_service_config = Service::new(
        Some(service_version.clone()),
        environment,
        Some(apm_language),
        Some(apm_runtime),
        None,
        Some(apm_service_node),
    );
    
    let mut apm_config = Config::new(apm_server_url)
        .with_service(apm_service_config)
        .with_system(apm_system);
    
    if let Some(token) = apm_secret_token {
        let auth = Authorization::SecretToken(token);
        apm_config = apm_config.with_authorization(auth);
        println!("🔒 Using Secret Token.");
    } else {
        println!("⚠️ No Authorization Found for APM.");
    }

    let apm_layer = tracing_elastic_apm::new_layer(service_name, apm_config)?;

    // 3. Inisialisasi Global Subscriber
    //
    // WARNING: EnvFilter::from_default_env() installs an ERROR-ONLY filter when RUST_LOG
    // is unset or empty. #[instrument] spans are INFO, so that silently disables every
    // span callsite in the process and NOTHING is ever sent to APM. Always fall back to
    // an explicit usable filter.
    //
    // WARNING: this EnvFilter MUST stay a global registry layer. Do NOT move it onto
    // apm_layer via `.with_filter(...)`: tracing-elastic-apm 3.4.0 panics with
    // "Trace context not found!" (layer.rs:65) whenever a child span reaches ApmLayer
    // while its parent was filtered out of ApmLayer only.
    //
    // WARNING: never set a bare level here or in RUST_LOG (e.g. RUST_LOG=debug). That
    // enables the reqwest/hyper/tokio callsites *inside* the APM exporter itself, whose
    // spans are then exported, which emits more requests -- a self-amplifying loop.
    // Keep csml_engine at `info`: the private high-fan-out DB helpers are deliberately
    // registered at `debug` and the crate does one detached HTTP POST per span.
    const DEFAULT_FILTER: &str = "warn,csml_server=info,csml_engine=info,csml_interpreter=warn,hyper=off,h2=off,reqwest=off,tokio=off,rustls=off";

    let filter_directives = match std::env::var("RUST_LOG") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => DEFAULT_FILTER.to_string(),
    };
    println!("🔎 Tracing filter: {}", filter_directives);

    let env_filter = EnvFilter::try_new(&filter_directives)
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let result = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(apm_layer)
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

            println!("✅ Elastic APM Tracing Ready for sending traces.");
            Ok(true)
        }
        Err(e) => {
            println!("⚠️ Tracing init returned: {} (subscriber may already be installed)", e);
            Ok(false)
        }
    }
}

fn detect_container() -> Option<Container> {
    if std::env::var("CONTAINER_NAME").is_ok() || std::path::Path::new("/.dockerenv").exists() {
        
        let container_id = std::env::var("HOSTNAME")
            .unwrap_or_else(|_| { 
                eprintln!("⚠️: HOSTNAME not found. Using UUID.");
                uuid::Uuid::new_v4().to_string() 
            });

        Some(Container {
            id: container_id,
        })
    } else {
        None
    }
}

fn detect_kubernetes() -> Option<Kubernetes> {
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        let pod_detail = Some(Pod {
            name: std::env::var("HOSTNAME").ok(),
            uid: std::env::var("KUBE_POD_UID").ok(), 
        });

        let node_detail = Some(Node {
            name: std::env::var("KUBE_NODE_NAME").ok(), 
        });
        
        Some(Kubernetes {
            namespace: std::env::var("KUBE_NAMESPACE").ok(),
            pod: pod_detail,
            node: node_detail,
        })
    } else {
        None
    }
}