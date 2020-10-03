#[derive(Deserialize, Debug)]
pub struct WsMsg {
    pub msg: String,
}

#[derive(Serialize, Debug)]
pub struct WsFrontMsg {
    pub msg: String,
    pub user_name: String,
}

pub struct Msg {
    pub msg: String,
    pub connection_id: u32,
    pub room_name: String,
}

#[derive(Deserialize, Debug)]
pub struct WsLogin {
    pub room_name: String,
    pub token: String,
    pub name: String,
}

pub struct Login {
    pub room_name: String,
    pub token: String,
    pub connection_id: u32,
    pub name: String,
}

pub struct Terminate {
    pub room_name: String,
    pub connection_id: u32,
}

#[derive(Deserialize, Debug)]
pub enum WsData {
    Message(WsMsg),
    Login(WsLogin),
}

pub enum Data {
    Message(Msg),
    Login(Login),
    Terminate(Terminate),
}
