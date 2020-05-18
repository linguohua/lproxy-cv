use crate::lws::WMessage;
use futures_03::sync::mpsc::UnboundedSender;
use futures_03::task::Task;
use log::error;
use nix::sys::socket::{shutdown, Shutdown};
use std::fmt;
use std::os::unix::io::RawFd;
use stream_cancel::Trigger;

pub struct XRequest {
    pub index: u16,
    pub tag: u16,
    pub is_inused: bool,
    pub request_tx: Option<UnboundedSender<WMessage>>,
    pub trigger: Option<Trigger>,

    pub wait_task: Option<Task>,

    pub rawfd: Option<RawFd>,
}

impl XRequest {
    pub fn new(idx: u16) -> XRequest {
        XRequest {
            index: idx,
            tag: 0,
            request_tx: None,
            trigger: None,
            is_inused: false,
            wait_task: None,
            rawfd: None,
        }
    }
}

impl fmt::Debug for XRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "XReq {{ indx: {}, tag: {} }}", self.index, self.tag,)
    }
}

pub struct XReqq {
    pub elements: Vec<XRequest>,
}

impl XReqq {
    pub fn new(size: usize) -> XReqq {
        let mut elements = Vec::with_capacity(size);
        for n in 0..size {
            elements.push(XRequest::new(n as u16));
        }

        XReqq { elements: elements }
    }

    pub fn alloc(&mut self, req_idx: u16, req_tag: u16) {
        let elements = &mut self.elements;
        if (req_idx as usize) >= elements.len() {
            error!("[XReq] alloc failed, req_idx exceed");
            return;
        }

        let req = &mut elements[req_idx as usize];
        XReqq::clean_req(req);

        req.tag = req_tag;
        // req.ipv4_le = ip;
        // req.port_le = port;
        req.request_tx = None;
        req.trigger = None;
        req.is_inused = true;
    }

    pub fn free(&mut self, idx: u16, tag: u16) -> bool {
        let elements = &mut self.elements;
        if idx as usize >= elements.len() {
            return false;
        }

        let req = &mut elements[idx as usize];

        if req.tag != tag {
            return false;
        }

        if !req.is_inused {
            return false;
        }

        XReqq::clean_req(req);

        true
    }

    pub fn clear_all(&mut self) {
        let elements = &mut self.elements;
        for e in elements.iter_mut() {
            XReqq::clean_req(e);
        }
    }

    fn clean_req(req: &mut XRequest) {
        // debug!("[Reqq]clean_req:{:?}", req.tag);
        req.request_tx = None;
        req.trigger = None;
        req.is_inused = false;

        if req.wait_task.is_some() {
            let wait_task = req.wait_task.take().unwrap();
            wait_task.notify();
        }

        if req.rawfd.is_some() {
            let rawfd = req.rawfd.take().unwrap();
            let r = shutdown(rawfd, Shutdown::Both);
            match r {
                Err(e) => {
                    error!("[XReq]close_rawfd failed:{}", e);
                }
                _ => {}
            }
        }
    }
}
