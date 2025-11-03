use tracing_elastic_apm::config::{Config, Service, Authorization};
use tracing_elastic_apm::model::{Language, Runtime};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tracing_elastic_apm::model::{System, Container, Kubernetes, Pod, Node, ServiceNode};

pub fn init_apm() -> Result<(), Box<dyn std::error::Error>> {
    let apm_server_url = std::env::var("ELASTIC_APM_SERVER_URL").ok();
    let service_name = std::env::var("ELASTIC_APM_SERVICE_NAME").ok();

    if apm_server_url.is_none() || service_name.is_none() {
        println!("⚠️ Variabel APM (URL atau SERVICE_NAME) not found. Skipping init APM.");
        return Ok(());
    }

    let apm_server_url = apm_server_url.unwrap();
    let service_name = service_name.unwrap();

    let apm_secret_token = std::env::var("ELASTIC_APM_SECRET_TOKEN").ok(); 
    let service_version = std::env!("CARGO_PKG_VERSION").to_owned();
    let environment = Some(std::env::var("ELASTIC_APM_ENVIRONMENT").unwrap_or_else(|_| "development".to_string()));
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
    let result = tracing_subscriber::registry()
        .with(apm_layer)
        .try_init();

    match result {
        Ok(_) => println!("✅ Elastic APM Tracing Ready for sending traces."),
        Err(_) => println!("⚠️ Tracing already initialized."),
    }

    Ok(())
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