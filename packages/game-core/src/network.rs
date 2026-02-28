use std::cell::RefCell;

thread_local! {
    static INCOMING: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

pub struct NetworkState {
    #[allow(dead_code)]
    pub game_id: String,
    #[allow(dead_code)]
    pub player_id: String,
    pub connected: bool,
    /// My index in playerOrder — equals my team number (player 0 = team 0, etc.)
    pub my_player_index: Option<usize>,
    /// Player names from playerOrder (index = team/player number)
    pub player_names: Vec<String>,
    /// Which players are bots
    pub player_is_bot: Vec<bool>,
}

impl NetworkState {
    pub fn new() -> Self {
        NetworkState {
            game_id: String::new(),
            player_id: String::new(),
            connected: false,
            my_player_index: None,
            player_names: Vec::new(),
            player_is_bot: Vec::new(),
        }
    }

    pub fn my_team(&self) -> Option<u32> {
        self.my_player_index.map(|i| i as u32)
    }

    pub fn poll_messages(&self) -> Vec<String> {
        INCOMING.with(|q| {
            let mut q = q.borrow_mut();
            std::mem::take(&mut *q)
        })
    }

    pub fn send_message(&self, msg: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            let bytes = msg.as_bytes();
            unsafe {
                js_send_ws(bytes.as_ptr(), bytes.len() as u32);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = msg;
        }
    }

    /// Dispatch a UI event (hit, died, turn_start, game_over) to the JS layer.
    /// The JS side listens and converts this into React toast notifications.
    pub fn send_game_event(&self, event_json: &str) {
        #[cfg(target_arch = "wasm32")]
        {
            let bytes = event_json.as_bytes();
            unsafe {
                js_game_event(bytes.as_ptr(), bytes.len() as u32);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = event_json;
        }
    }
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn js_send_ws(ptr: *const u8, len: u32);
    fn js_game_event(ptr: *const u8, len: u32);
}

#[no_mangle]
pub extern "C" fn alloc_buffer(len: u32) -> *mut u8 {
    let mut buf = Vec::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn on_ws_message(ptr: *const u8, len: u32) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    if let Ok(s) = std::str::from_utf8(slice) {
        INCOMING.with(|q| q.borrow_mut().push(s.to_string()));
    }
}

#[no_mangle]
pub extern "C" fn on_game_init(ptr: *const u8, len: u32) {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    if let Ok(s) = std::str::from_utf8(slice) {
        INCOMING.with(|q| {
            q.borrow_mut().push(format!("{{\"type\":\"init\",\"data\":{}}}", s));
        });
    }
}
