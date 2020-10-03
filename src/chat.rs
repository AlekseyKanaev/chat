use crate::repository::{MessageData, MsgParams as repoMsgParams, Repository, TokenData};
use message::Msg;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver as mpscReceiver, Sender as mpscSender};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use ws::{Builder, CloseCode, Handler, Handshake, Message, Result, Sender, Settings};

pub mod message;

const DEFAULT_PAGE_SIZE: i64 = 30;
const DEFAULT_PAGE_INDEX: i64 = 0;
const WS_MAX_CONNECTIONS: usize = 60_000;

pub struct Chat {
    repository: Arc<Mutex<Box<dyn Repository>>>,
    params: Params,
    ws_server: Arc<Mutex<Server>>,
}

struct Server {
    connections: HashMap<String, HashMap<u32, Client>>,
    user_names: HashMap<u32, String>,
    init_pool: HashMap<u32, Client>,
}

impl Default for Server {
    fn default() -> Self {
        let connections = HashMap::new();
        let init_pool = HashMap::new();
        let user_names = HashMap::new();

        Server {
            connections,
            init_pool,
            user_names,
        }
    }
}

struct Client {
    sender: Sender,
    addr: String,
    connection_id: u32,
    room_name: String,
}

struct WsHandler {
    sender: Sender,
    addr: String,
    room_name: String,
    client_tx: mpsc::Sender<Client>,
    data_tx: mpsc::Sender<message::Data>,
    id: u32,
}

impl WsHandler {
    fn terminate_connection(&self) {
        let terminate_conn = message::Data::Terminate(message::Terminate {
            connection_id: self.id,
            room_name: self.room_name.clone(),
        });

        match self.data_tx.send(terminate_conn) {
            Ok(_) => {}
            Err(e) => {
                error!("sending data by channel error: {}", e);
            }
        }
    }
}

impl Handler for WsHandler {
    fn on_shutdown(&mut self) {
        info!("Handler received WebSocket shutdown request.");
        self.terminate_connection();
    }

    fn on_open(&mut self, shake: Handshake) -> Result<()> {
        if let Ok(addr_opt) = shake.remote_addr() {
            let addr = match addr_opt {
                Some(a) => a,
                None => String::from("Unknown"),
            };

            info!("Connection with {} now open", addr);
            self.addr = addr.clone();

            let client = Client {
                sender: self.sender.clone(),
                addr,
                connection_id: self.id,
                room_name: String::from("Unassigned"),
            };

            match self.client_tx.send(client) {
                Ok(_) => {}
                Err(e) => {
                    error!("sending data by channel error: {}", e);
                }
            };
        }

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        debug!("Server got message '{}' from client {}. ", msg, self.addr);

        let ws_data_str = match msg.as_text() {
            Ok(str) => str,
            Err(e) => {
                error!("on_message error: {}", e);
                return Ok(());
            }
        };
        let ws_data: message::WsData = match serde_json::from_str(ws_data_str) {
            Ok(d) => d,
            Err(e) => {
                error!("on_message error: {}", e);
                return Ok(());
            }
        };

        let data: message::Data = match ws_data {
            message::WsData::Message(m) => message::Data::Message(message::Msg {
                msg: m.msg,
                connection_id: self.id,
                room_name: self.room_name.clone(),
            }),
            message::WsData::Login(l) => {
                self.room_name = l.room_name.clone();
                message::Data::Login(message::Login {
                    connection_id: self.id,
                    room_name: l.room_name,
                    token: l.token,
                    name: l.name,
                })
            }
        };

        match self.data_tx.send(data) {
            Ok(_) => {}
            Err(e) => {
                error!("sending data by channel error: {}", e);
            }
        }
        Ok(())
    }

    fn on_close(&mut self, code: ws::CloseCode, reason: &str) {
        info!("Connection closing due to ({:?}) {}", code, reason);
        self.terminate_connection();
    }
}

pub struct Params {
    pub(crate) ws_address: String,
}

pub fn new(params: Params, repository: Arc<Mutex<Box<dyn Repository>>>) -> Chat {
    let s = Server::default();
    let ws_server = Arc::new(Mutex::new(s));

    Chat {
        ws_server,
        params,
        repository,
    }
}

impl Chat {
    pub fn start(&self) {
        let (client_tx, client_rx): (mpscSender<Client>, mpscReceiver<Client>) = mpsc::channel();
        let (msg_tx, msg_rx): (mpscSender<message::Data>, mpscReceiver<message::Data>) =
            mpsc::channel();

        self.listen_ws(client_tx.clone(), msg_tx.clone());
        self.handle_ws_client(client_rx);
        self.handle_ws_data(msg_rx);
    }

    fn listen_ws(&self, client_tx: mpscSender<Client>, data_tx: mpscSender<message::Data>) {
        {
            let c_tx = client_tx;
            let d_tx = data_tx;
            let ws_addr = self.params.ws_address.clone();

            thread::spawn(move || {
                let mut connection_id = 0;
                let res = Builder::new()
                    .with_settings(Settings {
                        max_connections: WS_MAX_CONNECTIONS,
                        ..Settings::default()
                    })
                    .build(|out: Sender| {
                        connection_id += 1;

                        WsHandler {
                            room_name: String::from("not initiated"),
                            sender: out,
                            client_tx: c_tx.clone(),
                            data_tx: d_tx.clone(),
                            addr: String::new(),
                            id: connection_id,
                        }
                    })
                    .unwrap()
                    .listen(ws_addr);

                match res {
                    Ok(_) => {}
                    Err(e) => {
                        error!("error starting websocket service: {}", e);
                    }
                }
            });
        }
    }
    fn handle_ws_client(&self, client_rx: mpscReceiver<Client>) {
        {
            let client_rx = client_rx;
            let ws_server = self.ws_server.clone();
            thread::spawn(move || loop {
                let cl = client_rx.recv();
                {
                    match cl {
                        Ok(client) => {
                            let mut server = match ws_server.lock() {
                                Ok(r) => r,
                                Err(e) => {
                                    error!("error while getting lock on server: {}", e);
                                    continue;
                                }
                            };
                            info!("Client connected with addr:{}", client.addr);

                            server.init_pool.insert(client.connection_id, client);

                            let count = server.connections.keys().len();
                            debug!("hashmap size after adding client:{}", count);
                        }
                        Err(e) => {
                            error!("receiving client error: {}", e);
                        }
                    };
                }
            });
        }
    }

    fn broadcast(server: &Server, room_name: String, user_name: String, message: &Msg) {
        debug!("getting connections of room: {}", room_name);
        let connections_res = server.connections.get(&room_name);
        match connections_res {
            Some(connections) => {
                let front_msg = message::WsFrontMsg {
                    user_name,
                    msg: message.msg.clone(),
                };

                let ws_msg_res = serde_json::to_string(&front_msg);
                let ws_msg_opt = match ws_msg_res {
                    Ok(msg) => Some(msg),
                    Err(e) => {
                        error!("error while inserting message to db: {}", e);
                        None
                    }
                };
                if let Some(ws_msg) = ws_msg_opt {
                    for (id, s) in connections.iter() {
                        if *id != message.connection_id {
                            let send_res = s.sender.send(ws_msg.clone().as_str());
                            match send_res {
                                Ok(_) => debug!("sent msg to {}", s.addr),
                                Err(e) => error!("error while inserting message to db: {}", e),
                            }
                        }
                    }
                }
            }
            None => {}
        }
    }

    fn handle_message(
        msg: message::Msg,
        ws_server: &Arc<Mutex<Server>>,
        rep_mtx: &Arc<Mutex<Box<dyn Repository>>>,
    ) {
        debug!("Msg received");
        let server = match ws_server.lock() {
            Ok(r) => r,
            Err(e) => {
                error!("error while getting lock on server: {}", e);
                return;
            }
        };

        let count = server.connections.keys().len();
        debug!("hashmap size:{}", count);

        if let Some(user_name) = server.user_names.get(&msg.connection_id).clone() {
            let rep = match rep_mtx.lock() {
                Ok(r) => r,
                Err(e) => {
                    error!("error while getting lock on repository: {}", e);
                    return;
                }
            };

            let message_r = rep.message();
            let m_msg = MessageData {
                message: msg.msg.clone(),
                user_name: user_name.clone(),
                room_name: msg.room_name.clone(),
            };
            let insert_res = message_r.insert(m_msg);
            match insert_res {
                Ok(_) => {}
                Err(e) => error!("error while inserting message to db: {}", e),
            }

            Chat::broadcast(&server, msg.room_name.clone(), user_name.clone(), &msg);
        } else {
            error!("could not get name of user")
        }
    }

    fn handle_login(
        login: message::Login,
        ws_server: &Arc<Mutex<Server>>,
        rep_mtx: &Arc<Mutex<Box<dyn Repository>>>,
    ) {
        debug!("Login received");
        let repo = match rep_mtx.lock() {
            Ok(r) => r,
            Err(e) => {
                error!("error while getting lock on repository: {}", e);
                return;
            }
        };

        let token_r = repo.token();

        let mut server = match ws_server.lock() {
            Ok(r) => r,
            Err(e) => {
                error!("error while getting lock on server: {}", e);
                return;
            }
        };
        match token_r.get_valid(TokenData {
            token: login.token.as_str(),
            room_name: login.room_name.as_str(),
        }) {
            Ok(true) => {
                let client_res = server.init_pool.remove(&login.connection_id);
                if let Some(mut client) = client_res {
                    client.room_name = login.room_name.clone();
                    server.user_names.insert(login.connection_id, login.name);

                    let message_r = repo.message();

                    let params = repoMsgParams {
                        page: DEFAULT_PAGE_INDEX,
                        room_name: String::from(client.room_name.clone()),
                        size: DEFAULT_PAGE_SIZE,
                    };

                    let messages = message_r.get(params);
                    match messages {
                        Ok(messages) => {
                            for m in messages {
                                let front_msg = message::WsFrontMsg {
                                    user_name: m.user_name.clone(),
                                    msg: m.message.clone(),
                                };

                                if let Ok(ws_msg) = serde_json::to_string(&front_msg) {
                                    debug!("sending: {}", ws_msg);
                                    match client.sender.send(ws_msg) {
                                        Ok(_) => {}
                                        Err(e) => error!("sending to web socket error: {}", e),
                                    }
                                    thread::sleep(Duration::from_millis(100)); // flutter ws can not handle messages without pause
                                }
                            }
                        }
                        Err(e) => error!("could not get messages from DB: {}", e),
                    }

                    let mut room_res = server.connections.get_mut(client.room_name.as_str());
                    let room_key = client.room_name.clone();
                    match room_res.as_mut() {
                        Some(room) => {
                            let count = room.len();
                            info!(
                                "number of connections for room {} is: {}",
                                client.room_name, count
                            );

                            room.insert(client.connection_id, client);
                            info!("adding to by room_key: {}", room_key);
                        }
                        None => {
                            let mut room = HashMap::new();
                            let room_key = client.room_name.clone();
                            room.insert(client.connection_id, client);

                            info!("inserting by room_key: {}", room_key);

                            server.connections.insert(room_key, room);
                        }
                    }
                } else {
                    error!("could not get client from map");
                }
            }
            Ok(false) => {
                let client_res = server.init_pool.remove(&login.connection_id);
                match client_res {
                    Some(client) => match client.sender.close(CloseCode::Status) {
                        Ok(_) => {}
                        Err(e) => error!("closing socket error: {}", e),
                    },
                    None => error!("could not get client from map"),
                }
            }
            Err(e) => error!("login err: {}", e),
        };

        let del_res = token_r.delete(TokenData {
            token: login.token.as_str(),
            room_name: login.room_name.as_str(),
        });
        match del_res {
            Err(e) => {
                warn!("error while deleting token after login {}", e);
            }
            Ok(_) => {}
        }
    }

    fn handle_terminate(terminate: message::Terminate, ws_server: &Arc<Mutex<Server>>) {
        let mut server = match ws_server.lock() {
            Ok(r) => r,
            Err(e) => {
                error!("error while getting lock on server: {}", e);
                return;
            }
        };

        match server.connections.get_mut(terminate.room_name.as_str()) {
            Some(room_connections) => match room_connections.remove(&terminate.connection_id) {
                Some(_) => debug!(
                    "successfully removed connection: {} from room {}",
                    terminate.connection_id,
                    terminate.room_name.as_str()
                ),
                None => warn!(
                    "could not get connections for room: {}",
                    terminate.room_name.as_str()
                ),
            },
            None => warn!(
                "could not get connections for room: {}",
                terminate.room_name.as_str()
            ),
        }
    }

    fn handle_ws_data(&self, msg_rx: mpscReceiver<message::Data>) {
        {
            let msg_rx = msg_rx;
            let ws_server = self.ws_server.clone();
            let rep_mtx = self.repository.clone();

            thread::spawn(move || loop {
                match msg_rx.recv() {
                    Ok(data) => match data {
                        message::Data::Message(msg) => {
                            Chat::handle_message(msg, &ws_server, &rep_mtx);
                        }
                        message::Data::Login(login) => {
                            Chat::handle_login(login, &ws_server, &rep_mtx)
                        }
                        message::Data::Terminate(terminate) => {
                            Chat::handle_terminate(terminate, &ws_server)
                        }
                    },
                    Err(e) => {
                        println!("receiving data: {}", e);
                        break;
                    }
                };
            });
        }
    }
}
