use super::Request;
use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;

pub struct Reqq {
    pub elements: Vec<Request>,
    free: Vec<usize>,
}

impl Reqq {
    pub fn new(size: usize) -> Reqq {
        let mut elements = Vec::with_capacity(size);
        let mut free = Vec::with_capacity(size);
        for n in 0..size {
            elements.push(Request::new());
            free.push(size - 1 - n);
        }

        Reqq {
            elements: elements,
            free: free,
        }
    }

    pub fn alloc(&mut self, req_tx: &UnboundedSender<Bytes>) -> (u16, u16) {
        let free = &mut self.free;
        let elements = &mut self.elements;

        if free.len() < 1 {
            println!("alloc failed, no free slot in reqq");

            return (std::u16::MAX, std::u16::MAX);
        }

        let idx = free.pop().unwrap();
        let req = &mut elements[idx];
        req.tag = req.tag + 1;
        req.request_tx = Some(req_tx.clone());

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

        req.tag = req.tag + 1;
        req.request_tx = None;

        let free = &mut self.free;
        free.push(idx as usize);

        true
    }

    pub fn clear_all(&mut self) {
        let elements = &mut self.elements;
        for e in elements.iter_mut() {
            e.request_tx = None;
            e.tag += 1;
        }

        // not alloc-able
        self.free.clear();
    }
}
