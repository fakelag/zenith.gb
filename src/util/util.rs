use crate::GbButton;

pub fn get_high(value: u16) -> u8 {
    (value >> 8) as u8
}

pub fn get_low(value: u16) -> u8 {
    (value & 0xFF) as u8
}

pub fn set_high(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF) | (u16::from(value) << 8);
    *dst
}

pub fn set_low(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF00) | u16::from(value);
    *dst
}

pub fn value(high: u8, low: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

pub fn calc_button_bits(buttons: &[bool; GbButton::GbButtonMax as usize], p1_val: u8) -> u8 {
    let select_buttons = (p1_val & (1 << 5)) == 0;
    let select_dpad = (p1_val & (1 << 4)) == 0;

    let button_bits: u8 = if select_buttons && select_dpad {
        0xF
    } else if select_buttons {
        let start = (!buttons[GbButton::GbButtonStart as usize] as u8) << 3;
        let select = (!buttons[GbButton::GbButtonSelect as usize] as u8) << 2;
        let b = (!buttons[GbButton::GbButtonB as usize] as u8) << 1;
        let a = (!buttons[GbButton::GbButtonA as usize] as u8) << 0;

        start | select | a | b
    } else if select_dpad {
        let down = (!buttons[GbButton::GbButtonDown as usize] as u8) << 3;
        let up = (!buttons[GbButton::GbButtonUp as usize] as u8) << 2;
        let left = (!buttons[GbButton::GbButtonLeft as usize] as u8) << 1;
        let right = (!buttons[GbButton::GbButtonRight as usize] as u8) << 0;

        down | up | left | right
    } else {
        0xF
    };

    return button_bits & 0xF;
}
