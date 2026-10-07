//! OSCQuery HTTP advertisement and VRChat OSC service discovery.
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::net::SocketAddr;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
struct Daemon(ServiceDaemon);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.shutdown();
    }
}
pub struct Query {
    pub candidates: Mutex<mpsc::Receiver<SocketAddr>>,
    daemon: Daemon,
    fullname: Vec<String>,
    http: JoinHandle<()>,
    browse: JoinHandle<()>,
}
pub async fn start(
    _generation: u64,
    osc_port: u16,
    _preferred: Option<SocketAddr>,
    _commands: mpsc::Sender<crate::api::Request>,
) -> Result<Query, String> {
    let listener = TcpListener::bind(("0.0.0.0", 0))
        .await
        .map_err(|e| e.to_string())?;
    let http_port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let daemon = Daemon(ServiceDaemon::new().map_err(|e| e.to_string())?);
    let hostname = format!("slimevr-{}.local.", std::process::id());
    let instance = format!("SlimeVR-Server-{http_port}");
    let mut names = vec![];
    for (service, port) in [
        ("_osc._udp.local.", osc_port),
        ("_oscjson._tcp.local.", http_port),
    ] {
        let info = ServiceInfo::new(
            service,
            &instance,
            &hostname,
            "",
            port,
            None::<std::collections::HashMap<String, String>>,
        )
        .map_err(|e| e.to_string())?
        .enable_addr_auto();
        names.push(info.get_fullname().into());
        daemon.0.register(info).map_err(|e| e.to_string())?;
    }
    let discovery = daemon
        .0
        .browse("_osc._udp.local.")
        .map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel(16);
    let browse = tokio::spawn(async move {
        while let Ok(event) = discovery.recv_async().await {
            if let ServiceEvent::ServiceResolved(service) = event {
                if !service.get_fullname().starts_with("VRChat-Client") {
                    continue;
                }
                for address in service.get_addresses() {
                    let socket = SocketAddr::new(address.to_ip_addr(), service.get_port());
                    if tx.send(socket).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    let http = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {accepted=listener.accept()=>{let Ok((mut stream,_))=accepted else{break;};if clients.len()>=16{continue;}let instance=instance.clone();clients.spawn(async move{let mut bytes=vec![0u8;8192];let mut length=0;let read=async{while length<bytes.len(){let n=stream.read(&mut bytes[length..]).await?;if n==0{break;}length+=n;if bytes[..length].windows(4).any(|v|v==b"\r\n\r\n"){break;}}Ok::<(),std::io::Error>(())};if !matches!(tokio::time::timeout(std::time::Duration::from_secs(2),read).await,Ok(Ok(()))){return;}let request=String::from_utf8_lossy(&bytes[..length]);let path=request.split_whitespace().nth(1).unwrap_or("/");let body=if path=="/?HOST_INFO"{serde_json::json!({"NAME":instance,"OSC_PORT":osc_port,"OSC_TRANSPORT":"UDP","EXTENSIONS":{"ACCESS":true,"VALUE":true,"RANGE":true,"DESCRIPTION":true}})}else{node(path)};let body=body.to_string();let response=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len());let _=tokio::time::timeout(std::time::Duration::from_secs(2),stream.write_all(response.as_bytes())).await;});},_=clients.join_next(),if !clients.is_empty()=>{}}
        }
    });
    Ok(Query {
        candidates: Mutex::new(rx),
        daemon,
        fullname: names,
        http,
        browse,
    })
}
fn node(path: &str) -> serde_json::Value {
    fn leaf(name: &str) -> serde_json::Value {
        serde_json::json!({"FULL_PATH":format!("/tracking/vrsystem/{name}/pose"),"DESCRIPTION":"VR system position and rotation","TYPE":"ffffff","ACCESS":2,"VALUE":[0,0,0,0,0,0]})
    }
    let mut root = serde_json::json!({"FULL_PATH":"/","CONTENTS":{"tracking":{"FULL_PATH":"/tracking","CONTENTS":{"vrsystem":{"FULL_PATH":"/tracking/vrsystem","CONTENTS":{}}}}}});
    for name in ["head", "leftwrist", "rightwrist"] {
        root["CONTENTS"]["tracking"]["CONTENTS"]["vrsystem"]["CONTENTS"][name] = serde_json::json!({"FULL_PATH":format!("/tracking/vrsystem/{name}"),"CONTENTS":{"pose":leaf(name)}});
    }
    let mut current = &root;
    for segment in path.trim_matches('/').split('/').filter(|s| !s.is_empty()) {
        current = &current["CONTENTS"][segment];
    }
    current.clone()
}
impl Drop for Query {
    fn drop(&mut self) {
        self.http.abort();
        self.browse.abort();
        for name in &self.fullname {
            let _ = self.daemon.0.unregister(name);
        }
    }
}
