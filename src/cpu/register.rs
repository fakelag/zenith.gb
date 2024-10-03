use crate::util::util;

pub struct Reg8b<'a> {
    addr: &'a mut u16,
    high: bool,
}

pub struct Reg16b<'a> {
    addr: &'a mut u16,
}

impl<'a> Reg8b<'a> {
    pub fn new(addr: &'a mut u16, high: bool) -> Reg8b<'a> {
        Self { addr, high }
    }

    pub fn set(&mut self, val: u8) {
        if self.high {
            util::set_high(self.addr, val);
        } else {
            util::set_low(self.addr, val);
        }
    }

    pub fn get(&self) -> u8 {
        if self.high {
            (*self.addr >> 8) as u8
        } else {
            (*self.addr & 0xFF) as u8
        }
    }
}

impl<'a> Reg16b<'a> {
    pub fn new(addr: &'a mut u16) -> Reg16b<'a> {
        Self { addr }
    }

    pub fn set(&mut self, val: u16) {
        *self.addr = val;
    }

    pub fn get(&mut self) -> u16 {
        *self.addr
    }

    pub fn dec(&mut self) -> u16 {
        let prev = *self.addr;
        *self.addr = self.addr.wrapping_sub(1);
        prev
    }

    pub fn inc(&mut self) -> u16 {
        let prev = *self.addr;
        *self.addr = self.addr.wrapping_add(1);
        prev
    }
}
