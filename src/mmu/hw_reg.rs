use super::mmu::MMU;

pub const HWR_P1: u16 = 0xFF00;
pub const HWR_SB: u16 = 0xFF01;
pub const HWR_SC: u16 = 0xFF02;
pub const HWR_DIV: u16 = 0xFF04;
pub const HWR_TIMA: u16 = 0xFF05;
pub const HWR_TMA: u16 = 0xFF06;
pub const HWR_TAC: u16 = 0xFF07;
pub const HWR_IF: u16 = 0xFF0F;
pub const HWR_NR10: u16 = 0xFF10;
pub const HWR_NR11: u16 = 0xFF11;
pub const HWR_NR12: u16 = 0xFF12;
pub const HWR_NR13: u16 = 0xFF13;
pub const HWR_NR14: u16 = 0xFF14;
pub const HWR_NR21: u16 = 0xFF16;
pub const HWR_NR22: u16 = 0xFF17;
pub const HWR_NR23: u16 = 0xFF18;
pub const HWR_NR24: u16 = 0xFF19;
pub const HWR_NR30: u16 = 0xFF1A;
pub const HWR_NR31: u16 = 0xFF1B;
pub const HWR_NR32: u16 = 0xFF1C;
pub const HWR_NR33: u16 = 0xFF1D;
pub const HWR_NR34: u16 = 0xFF1E;
pub const HWR_NR41: u16 = 0xFF20;
pub const HWR_NR42: u16 = 0xFF21;
pub const HWR_NR43: u16 = 0xFF22;
pub const HWR_NR44: u16 = 0xFF23;
pub const HWR_NR50: u16 = 0xFF24;
pub const HWR_NR51: u16 = 0xFF25;
pub const HWR_NR52: u16 = 0xFF26;
pub const HWR_LCDC: u16 = 0xFF40;
pub const HWR_STAT: u16 = 0xFF41;
pub const HWR_LY: u16 = 0xFF44;
pub const HWR_SCY: u16 = 0xFF42;
pub const HWR_SCX: u16 = 0xFF43;
pub const HWR_LYC: u16 = 0xFF45;
pub const HWR_DMA: u16 = 0xFF46;
pub const HWR_BGP: u16 = 0xFF47;
pub const HWR_OBP0: u16 = 0xFF48;
pub const HWR_OBP1: u16 = 0xFF49;
pub const HWR_WY: u16 = 0xFF4A;
pub const HWR_WX: u16 = 0xFF4B;

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

    pub fn inc(&mut self) -> u8 {
        let prev = self.mmu.bus_read(self.addr);
        self.mmu.bus_write(self.addr, prev + 1);
        prev
    }

    pub fn dec(&mut self) -> u8 {
        let prev = self.mmu.bus_read(self.addr);
        self.mmu.bus_write(self.addr, prev - 1);
        prev
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
