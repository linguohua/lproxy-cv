use super::Request;

pub struct Reqq {
    elements: Vec<Option<Request>>,
    free: Vec<usize>,
}

impl Reqq {
    pub fn new(size: usize) -> Reqq {
        let mut elements = Vec::with_capacity(size);
        let mut free = Vec::with_capacity(size);
        for n in 0..size {
            elements.push(None);
            free.push(size - 1 - n);
        }

        Reqq {
            elements: elements,
            free: free,
        }
    }

    pub fn alloc(&mut self, req: Request) -> usize {
        let free = &mut self.free;
        let elements = &mut self.elements;

        if free.len() < 1 {
            println!("alloc failed, no free slot in reqq");

            return std::usize::MAX;
        }

        let idx = free.pop().unwrap();
        let mut req = req;
        req.bind(idx);

        elements[idx] = Some(req);

        idx
    }

    pub fn free(&mut self, idx: usize) {
        let free = &mut self.free;
        let elements = &mut self.elements;

        if free.len() < 1 {
            println!("alloc failed, no free slot in reqq");

            return;
        }

        free.push(idx);
        elements[idx] = None;
    }
}
