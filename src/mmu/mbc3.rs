use crate::{cartridge::cartridge::Cartridge, mmu::mbc};

use super::mmu;

const GB_CLOCKS_PER_SECOND: u32 = 4_194_304 / 4;

const RTC_S: usize = 0;
// const RTC_M: usize = 1;
const RTC_H: usize = 2;
const RTC_DL: usize = 3;
const RTC_DH: usize = 4;

pub struct MBC3 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    // 7-bit rom bank number
    rom_bank: usize,
    // 2-bit ram bank number
    ram_bank: Option<usize>,
    ram_enabled: bool,

    num_rom_banks: usize,
    num_ram_banks: usize,

    has_rtc: bool,
    rtc_registers: [u8; 5],
    rtc_latch: Option<[u8; 5]>,
    rtc_select: Option<usize>,
    rtc_latch_next: bool,
    rtc_cycles_left: u32,

    save_path: Option<String>,
}

impl MBC3 {
    pub fn new() -> MBC3 {
        Self {
            rom: Vec::new(),
            ram: Vec::new(),
            rom_bank: 1,
            ram_bank: None,
            num_rom_banks: 1,
            num_ram_banks: 1,
            ram_enabled: false,
            has_rtc: false,
            rtc_registers: [0; 5],
            rtc_latch: None,
            rtc_select: None,
            rtc_latch_next: false,
            rtc_cycles_left: GB_CLOCKS_PER_SECOND,
            save_path: None,
        }
    }

    fn rtc_register_mask(rtc_select: usize) -> u8 {
        return match rtc_select {
            0..=1   => 0b111111,
            2       => 0b11111,
            3       => 0b11111111,
            4       => 0b11000001,
            _       => unreachable!("invalid register"),
        };
    }

    fn rtc_rollover(rtc_select: usize) -> u16 {
        return match rtc_select {
            0..=1   => 60,
            2       => 24,
            3       => 0x1FF,
            4       => unreachable!("dh does not have rollover"),
            _       => unreachable!("invalid register"),
        };
    }

    fn rtc_inc(&mut self, rtc_select: usize) {
        match rtc_select {
            RTC_S..=RTC_H => {
                let rtc_reg = &mut self.rtc_registers[rtc_select];

                *rtc_reg = rtc_reg.wrapping_add(1);

                if *rtc_reg & MBC3::rtc_register_mask(rtc_select) == 0 {
                    *rtc_reg = 0;
                } else if MBC3::rtc_rollover(rtc_select) == (*rtc_reg).into() {
                    *rtc_reg = 0;
                    self.rtc_inc(rtc_select + 1);
                }
            }
            RTC_DL => {
                let mut day_counter: u16 = (u16::from(self.rtc_registers[RTC_DH]) << 8) | u16::from(self.rtc_registers[RTC_DL]);
                day_counter = day_counter.wrapping_add(1);
    
                if day_counter & (1 << 9) != 0 {
                    // overflow
                    day_counter = 1 << 15               // day counter carry bit
                        | (day_counter & (1 << 14));    // halt bit
                }
    
                self.rtc_registers[RTC_DH] = (day_counter >> 8).try_into().unwrap();
                self.rtc_registers[RTC_DL] = (day_counter & 0xFF).try_into().unwrap();
            }
            _ => unreachable!("invalid register"),
        }
    }
}

impl mmu::MBC for MBC3 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        let rom_banks = mbc::rom_banks(hdr);
        let ram_banks = mbc::ram_banks(hdr);

        debug_assert!(rom_banks.size_bytes == cartridge.data.len());

        self.rom = cartridge.data[0..rom_banks.size_bytes].to_vec();
        self.num_rom_banks = rom_banks.num_banks;

        self.ram = vec![0; ram_banks.size_bytes];
        self.num_ram_banks = ram_banks.num_banks;

        self.has_rtc = [0x0F, 0x10].contains(&cartridge.header.cart_type);

        match hdr.cart_type {
            0x10 | 0x13 => {
                if let Ok(save_path) = mbc::save_file_from_rom_path(&cartridge.rom_path) {
                    mbc::read_save(&save_path, &mut self.ram);
                    self.save_path = Some(save_path);
                }
            }
            _ => {}
        }
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                return self.rom[usize::from(address)];
            }
            0x4000..=0x7FFF => {
                let rom_addr = 0x4000 * self.rom_bank + usize::from(address - 0x4000);
                return self.rom[rom_addr];
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return 0xFF;
                }

                if let Some(rtc_select) = self.rtc_select {
                    if let Some(rtc_latch) = self.rtc_latch {
                        return rtc_latch[rtc_select];
                    } else {
                        return 0xFF;
                    }
                }

                if let Some(ram_bank) = self.ram_bank {
                    let ram_addr = 0x2000 * ram_bank + usize::from(address - 0xA000);
                    return self.ram[ram_addr];
                }

                return 0xFF;
            }
            _ => {
                unreachable!();
            }
        }
    }

    fn write(&mut self, address: u16, data: u8) {
        match address {
            0x0..=0x1FFF => {
                // Note: Also enables access to RTC
                self.ram_enabled = data & 0xF == 0xA;
            }
            0x2000..=0x3FFF => {
                let bank_num = data & 0x7F;

                if bank_num == 0 {
                    self.rom_bank = 1;
                } else {
                    self.rom_bank = usize::from(bank_num) % self.num_rom_banks;
                }
            }
            0x4000..=0x5FFF => {
                match data {
                    0..=0x3 => {
                        self.ram_bank = Some(usize::from(data) % self.num_ram_banks);
                        self.rtc_select = None;
                    }
                    0x8..=0x0C => {
                        if self.has_rtc {
                            self.ram_bank = None;
                            self.rtc_select = Some((data - 0x8).into());
                        }
                    }
                    _ => {}
                }
            }
            0x6000..=0x7FFF => {
                if !self.has_rtc {
                    return;
                }

                if self.rtc_latch_next && data == 1 {
                    self.rtc_latch = Some(self.rtc_registers);
                }
                self.rtc_latch_next = data == 0;
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                if let Some(rtc_select) = self.rtc_select {
                    let mask = MBC3::rtc_register_mask(rtc_select);

                    let val_masked = data & mask;

                    if rtc_select == 0 {
                        self.rtc_cycles_left = GB_CLOCKS_PER_SECOND;
                    }

                    self.rtc_registers[rtc_select] = val_masked;

                    if let Some(rtc_latch) = &mut self.rtc_latch {
                        rtc_latch[rtc_select] = val_masked;
                    }
                    return;
                }

                if let Some(ram_bank) = self.ram_bank {
                    let ram_addr = 0x2000 * ram_bank + usize::from(address - 0xA000);
                    self.ram[ram_addr] = data;
                }
            }
            _ => {}
        }
    }

    fn step(&mut self, cycles: u8) {
        if !self.has_rtc {
            return;
        }

        if self.rtc_registers[RTC_DH] & (1 << 6) != 0 {
            return;
        }

        if self.rtc_cycles_left <= cycles.into() {
            self.rtc_inc(RTC_S);
            self.rtc_cycles_left = GB_CLOCKS_PER_SECOND - (u32::from(cycles) - self.rtc_cycles_left);
        } else {
            self.rtc_cycles_left =self.rtc_cycles_left.wrapping_sub(cycles.into());
        }
    }

    fn save(&mut self) {
        if let Some(save_path) = &self.save_path {
            _ = mbc::write_save(save_path, &self.ram);
        }
    }
}