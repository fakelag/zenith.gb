use super::mmu::MMU;

pub struct HwReg<'a> {
    addr: u16,
    mmu: &'a mut MMU,
}

impl<'a> HwReg<'a> {
    pub fn new(addr: u16, mmu: &'a mut MMU) -> HwReg<'a> {
        Self { addr, mmu }
    }

    pub fn set(&mut self, val: u8) {
        self.mmu.bus_write(self.addr, val);
    }

    pub fn get(&self) -> u8 {
        self.mmu.bus_read(self.addr)
    }

    pub fn check_bit(&self, bit: u8) -> bool {
        debug_assert!(bit < 8);
        (self.get() & (1 << bit)) != 0
    }

    pub fn set_bit(&mut self, bit: u8, set: bool) {
        let current_val = self.get();
    
        if set {
            self.set(current_val | (1 << bit));
        } else {
            self.set(current_val & !(1 << bit));
        }
    }
}
