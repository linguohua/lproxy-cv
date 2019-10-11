use super::Request;

use log::error;

use log::debug;

pub struct Reqq {
    pub elements: Vec<Request>,
    free: Vec<usize>,
}

impl Reqq {
    pub fn new(size: usize) -> Reqq {
        let mut elements = Vec::with_capacity(size);
        let mut free = Vec::with_capacity(size);
        for n in 0..size {
            elements.push(Request::new(n as u16));
            free.push(size - 1 - n);
        }

        Reqq {
            elements: elements,
            free: free,
        }
    }

    pub fn alloc(&mut self, req2: Request) -> (u16, u16) {
        let free = &mut self.free;
        let elements = &mut self.elements;

        if free.len() < 1 {
            error!("alloc failed, no free slot in reqq");

            return (std::u16::MAX, std::u16::MAX);
        }

        let idx = free.pop().unwrap();
        let req = &mut elements[idx];
        req.tag = req.tag + 1;
        req.request_tx = req2.request_tx;
        req.trigger = req2.trigger;

        (idx as u16, req.tag)
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

        Reqq::clean_req(req);

        let free = &mut self.free;
        free.push(idx as usize);

        true
    }

    pub fn clear_all(&mut self) {
        let elements = &mut self.elements;
        for e in elements.iter_mut() {
            Reqq::clean_req(e);
        }

        // not alloc-able
        self.free.clear();
    }

    fn clean_req(req: &mut Request) {
        debug!("[Reqq]clean_req:{:?}", req.tag);

        req.tag = req.tag + 1;
        req.request_tx = None;
        req.trigger = None;
    }
}
