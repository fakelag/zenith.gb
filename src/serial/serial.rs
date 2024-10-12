use crate::soc::{interrupt, soc};

use super::{printer::Printer, SerialConnector};

pub struct Serial {
    sb: u8,

    shift_counter: u8,
    incoming: Option<u8>,

    reg_enable: bool,
    reg_select: bool,

    printer: Box<Printer>,
}

impl Serial {
    pub fn new() -> Self {
        Self {
            sb: 0,
            reg_enable: false,
            reg_select: false,
            incoming: None,
            shift_counter: 0,
            printer: Box::new(Printer::new(".")),
        }
    }

    pub fn clock(&mut self, ctx: &mut soc::ClockContext) {
        if !self.reg_enable || !self.reg_select {
            return;
        }

        if self.shift_counter == 8 {
            self.finish_transmission(ctx);
            return;
        }

        if self.shift_counter == 0 {
            self.printer.as_mut().send_byte(self.sb);
            self.incoming = self.printer.as_mut().take();
        }

        if let Some(incoming) = self.incoming {
            let curr_bit = 7 - self.shift_counter;

            self.sb <<= 1;
            self.sb |= (incoming >> curr_bit) & 0x1;

            self.shift_counter += 1;
        }
    }

    fn finish_transmission(&mut self, ctx: &mut soc::ClockContext) {
        self.reg_enable = false;
        self.shift_counter = 0;
        self.incoming = None;
        ctx.set_interrupt(interrupt::INTERRUPT_BIT_SERIAL);
    }

    pub fn read_sb(&mut self) -> u8 {
        self.sb
    }

    pub fn write_sb(&mut self, data: u8) {
        self.sb = data;
    }

    pub fn read_sc(&mut self) -> u8 {
        ((self.reg_enable as u8) << 7) | (self.reg_select as u8) | 0x7E
    }

    pub fn write_sc(&mut self, data: u8) {
        // @todo CGB: Clock speed bit
        self.reg_enable = data & 0x80 != 0;
        self.reg_select = data & 0x1 != 0;
    }
}
