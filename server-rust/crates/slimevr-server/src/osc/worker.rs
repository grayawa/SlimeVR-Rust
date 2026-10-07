use super::{state::Output, Settings};
use crate::api::Request;
use std::{collections::BTreeSet, net::SocketAddr};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, watch},
    task::{JoinHandle, JoinSet},
};
#[derive(Debug)]
pub enum Event {
    Datagram {
        generation: u64,
        port: u16,
        bytes: Vec<u8>,
    },
    Error {
        generation: u64,
        message: String,
    },
    Query {
        generation: u64,
        address: SocketAddr,
    },
}
struct Send {
    generation: u64,
    output: Output,
}
pub struct Controller {
    configuration: watch::Sender<(u64, Settings)>,
    output: watch::Sender<Option<Send>>,
    task: JoinHandle<()>,
    config: Settings,
    generation: u64,
}
impl Controller {
    pub fn start(config: Settings, commands: mpsc::Sender<Request>) -> Self {
        let (configuration, receiver) = watch::channel((1, config.clone()));
        let (output, outputs) = watch::channel(None);
        let task = tokio::spawn(run(receiver, outputs, commands));
        Self {
            configuration,
            output,
            task,
            config,
            generation: 1,
        }
    }
    pub fn configure(&mut self, generation: u64, config: &Settings) {
        if self.config != *config {
            self.config = config.clone();
            self.generation = generation;
            self.output.send_replace(None);
            self.configuration
                .send_replace((generation, config.clone()));
        }
    }
    pub fn send(&self, output: Output) {
        self.output.send_replace(Some(Send {
            generation: self.generation,
            output,
        }));
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn error(commands: &mpsc::Sender<Request>, generation: u64, message: String) {
    let _ = commands
        .send(Request::Osc(Event::Error {
            generation,
            message,
        }))
        .await;
}
async fn destination(e: &super::config::Endpoint) -> Result<SocketAddr, String> {
    tokio::net::lookup_host((e.address.as_str(), e.port_out))
        .await
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("OSC hostname resolved to no address".into())
}
async fn run(
    mut configuration: watch::Receiver<(u64, Settings)>,
    mut outputs: watch::Receiver<Option<Send>>,
    commands: mpsc::Sender<Request>,
) {
    loop {
        let (generation, c) = configuration.borrow_and_update().clone();
        let mut tasks = JoinSet::new();
        let mut ports = BTreeSet::new();
        for e in [&c.router, &c.vrc.endpoint, &c.vmc.endpoint] {
            if e.enabled {
                ports.insert(e.port_in);
            }
        }
        let router = if c.router.enabled {
            match destination(&c.router).await {
                Ok(d) => Some(d),
                Err(e) => {
                    error(
                        &commands,
                        generation,
                        format!("OSC router destination: {e}"),
                    )
                    .await;
                    None
                }
            }
        } else {
            None
        };
        let vrc = if c.vrc.endpoint.enabled {
            match destination(&c.vrc.endpoint).await {
                Ok(d) => Some(d),
                Err(e) => {
                    error(
                        &commands,
                        generation,
                        format!("VRChat OSC destination: {e}"),
                    )
                    .await;
                    None
                }
            }
        } else {
            None
        };
        let vmc = if c.vmc.endpoint.enabled {
            match destination(&c.vmc.endpoint).await {
                Ok(d) => Some(d),
                Err(e) => {
                    error(&commands, generation, format!("VMC destination: {e}")).await;
                    None
                }
            }
        } else {
            None
        };
        for port in ports {
            match UdpSocket::bind(("0.0.0.0", port)).await {
                Ok(socket) => {
                    let commands = commands.clone();
                    let forward = if port == c.router.port_in {
                        router
                    } else {
                        None
                    };
                    tasks.spawn(async move {
                        let mut buffer = vec![0; 65536];
                        loop {
                            let Ok((length, from)) = socket.recv_from(&mut buffer).await else {
                                break;
                            };
                            if length > 8192 {
                                continue;
                            }
                            if let Some(to) = forward {
                                if from != to {
                                    let _ = socket.send_to(&buffer[..length], to).await;
                                }
                            }
                            if commands
                                .send(Request::Osc(Event::Datagram {
                                    generation,
                                    port,
                                    bytes: buffer[..length].into(),
                                }))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    });
                }
                Err(e) => error(&commands, generation, format!("OSC port {port}: {e}")).await,
            }
        }
        let query = if c.vrc.endpoint.enabled && c.vrc.oscquery_enabled {
            match super::query::start(generation, c.vrc.endpoint.port_in, vrc, commands.clone())
                .await
            {
                Ok(q) => Some(q),
                Err(e) => {
                    error(&commands, generation, format!("OSCQuery: {e}")).await;
                    None
                }
            }
        } else {
            None
        };
        let send4 = UdpSocket::bind(("0.0.0.0", 0)).await.ok();
        let send6 = UdpSocket::bind(("::", 0)).await.ok();
        let mut query_target = None;
        let mut last_send_error = None;
        loop {
            tokio::select! {changed=configuration.changed()=>{if changed.is_err(){return;}break;},changed=outputs.changed()=>{if changed.is_err(){return;}let packets=outputs.borrow_and_update().as_ref().filter(|o|o.generation==generation).map(|o|(o.output.vrc.clone(),o.output.vmc.clone()));if let Some((vrc_packet,vmc_packet))=packets{for(bytes,to)in[(vrc_packet,query_target.or(vrc)),(vmc_packet,vmc)]{if let(Some(bytes),Some(to))=(bytes,to){let socket=if to.is_ipv4(){&send4}else{&send6};if let Some(socket)=socket{if let Err(e)=socket.send_to(&bytes,to).await{if last_send_error.is_none_or(|at:std::time::Instant|at.elapsed()>=std::time::Duration::from_secs(5)){last_send_error=Some(std::time::Instant::now());error(&commands,generation,format!("OSC send: {e}")).await;}}}}}}},candidate=async{match &query{Some(q)=>q.candidates.lock().await.recv().await,None=>std::future::pending().await}}=>{if let Some(to)=candidate{let prefer=|ip:std::net::IpAddr|ip.is_loopback()||if_addrs::get_if_addrs().is_ok_and(|interfaces|interfaces.iter().any(|i|i.ip()==ip));let matches=vrc.is_some_and(|v|v.ip()==to.ip()||(prefer(v.ip())&&prefer(to.ip())));let current_matches=query_target.is_some_and(|p:SocketAddr|vrc.is_some_and(|v|v.ip()==p.ip()||(prefer(v.ip())&&prefer(p.ip()))));if !current_matches&&(query_target.is_none()||matches||vrc.is_some_and(|v|v.port()==to.port()&&query_target.is_some_and(|p|p.port()!=v.port()))){query_target=Some(to);}}},_=tasks.join_next(),if !tasks.is_empty()=>{}}
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        drop(query);
    }
}
