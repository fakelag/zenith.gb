use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::util::util;

use super::SerialConnector;

const PRINTER_PALETTE: [(u8, u8, u8); 4] = [
    (0x88, 0xa0, 0x48),
    (0x48, 0x68, 0x30),
    (0x28, 0x40, 0x20),
    (0x18, 0x28, 0x08),
];

#[derive(Clone, Copy)]
enum PrinterRecvState {
    RecvWaitMagic,
    RecvWaitCommand,
    RecvWaitCompression,
    RecvWaitLength,
    RecvWaitData,
    RecvWaitChecksum,
    RecvWaitKeepAlive,
    RecvWaitStatus,
}

#[derive(PartialEq, Clone, Copy)]
enum PrinterStatus {
    PrinterIdle = 0,
    PrinterReady = 0x8,
    PrinterPrinting = 0x6,
    PrinterDone = 0x4,
}

pub struct Printer {
    recv_state: PrinterRecvState,
    recv_data: Vec<u8>,
    status: PrinterStatus,
    recv_data_length: u16,
    ram: Vec<u8>,
    compression: bool,
    output: Option<u8>,
    image_path: String,
}

impl Printer {
    pub fn new(image_path: &str) -> Self {
        Self {
            ram: vec![0; 0],
            recv_data: vec![0; 0],
            recv_state: PrinterRecvState::RecvWaitMagic,
            status: PrinterStatus::PrinterIdle,
            recv_data_length: 0,
            compression: false,
            output: None,
            image_path: image_path.to_string(),
        }
    }
}

impl Printer {
    fn reset(&mut self) {
        self.ram = vec![0; 0];
        self.recv_data = vec![0; 0];
        self.recv_state = PrinterRecvState::RecvWaitMagic;
        self.status = PrinterStatus::PrinterIdle;
        self.recv_data_length = 0;
        self.output = None;
    }

    fn print(&mut self, _num_sheets: u8, _margins: u8, palette: u8, _exposure: u8) {
        let pic_height = self.ram.len() / 40;

        let mut rt: Box<[[u8; 160]]> = vec![[0; 160]; pic_height]
            .into_boxed_slice()
            .try_into()
            .unwrap();

        for y in 0..pic_height {
            let mut x = 0;
            while x < 160 {
                let x_coord = x / 8;
                let tile_index = (x_coord + 20 * (y / 8)) % 360;
                let line_offset = 2 * (y % 8);

                let tile_base = tile_index * 16 + line_offset;
                let (tile_lsb, tile_msb) = (self.ram[tile_base], self.ram[tile_base + 1]);

                for bit_idx in (0..8).rev() {
                    let hb = (tile_msb >> bit_idx) & 0x1;
                    let lb = (tile_lsb >> bit_idx) & 0x1;
                    let img_pixel = lb | (hb << 1);

                    let palette_color = (palette >> (img_pixel * 2)) & 0x3;

                    rt[y][x] = palette_color;
                    x += 1;
                }
            }
        }

        let mut bmp_img = bmp::Image::new(160, pic_height as u32);

        for x in 0..160 {
            for y in 0..pic_height {
                let gb_color = rt[y][x];
                let palette_color = PRINTER_PALETTE[gb_color as usize];
                bmp_img.set_pixel(
                    x as u32,
                    y as u32,
                    bmp::Pixel {
                        r: palette_color.0,
                        g: palette_color.1,
                        b: palette_color.2,
                    },
                );
            }
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time > UNIX_EPOCH")
            .as_millis();

        let mut path = PathBuf::from(self.image_path.clone());
        path.push(format!("printer-{timestamp}.bmp"));

        _ = bmp_img.save(path);
    }
}

impl SerialConnector for Printer {
    fn send_byte(&mut self, byte: u8) {
        self.recv_data.push(byte);

        let output = match (self.recv_state, self.recv_data.as_slice()) {
            (PrinterRecvState::RecvWaitMagic, [0x88, 0x33]) => {
                if self.status == PrinterStatus::PrinterDone {
                    self.reset();
                }
                if self.status == PrinterStatus::PrinterPrinting {
                    self.status = PrinterStatus::PrinterDone;
                }
                self.recv_state = PrinterRecvState::RecvWaitCommand;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitCommand, [0xF]) => {
                self.recv_state = PrinterRecvState::RecvWaitCompression;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitCommand, [0x1]) => {
                // Command 1: Clear printer RAM
                self.recv_state = PrinterRecvState::RecvWaitCompression;
                self.ram.clear();
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitCommand, [0x2]) => {
                // Command 2: Start printing
                self.status = PrinterStatus::PrinterPrinting;
                self.recv_state = PrinterRecvState::RecvWaitCompression;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitCommand, [0x4]) => {
                // Command 4: Fill buffer
                self.status = PrinterStatus::PrinterReady;
                self.recv_state = PrinterRecvState::RecvWaitCompression;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitCommand, _) => {
                panic!("unimplemented command {}", byte);
            }
            (PrinterRecvState::RecvWaitCompression, [use_compression]) => {
                self.recv_state = PrinterRecvState::RecvWaitLength;
                self.compression = *use_compression != 0;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitLength, [lsb, msb]) => {
                self.recv_data_length = util::value(*msb, *lsb);
                if self.recv_data_length == 0 {
                    self.recv_state = PrinterRecvState::RecvWaitChecksum;
                } else {
                    self.recv_state = PrinterRecvState::RecvWaitData;
                }
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitData, recv_buf) => {
                if recv_buf.len() < self.recv_data_length as usize {
                    0x0
                } else {
                    match self.status {
                        PrinterStatus::PrinterPrinting => {
                            let [num_sheets, margins, palette, exposure] = recv_buf else {
                                panic!("invalid recv buf length for printing");
                            };

                            self.print(*num_sheets, *margins, *palette, *exposure);
                        }
                        PrinterStatus::PrinterReady => {
                            self.ram.extend_from_slice(recv_buf);
                        }
                        _ => {}
                    }

                    self.recv_state = PrinterRecvState::RecvWaitChecksum;
                    self.recv_data.clear();
                    0x0
                }
            }
            (PrinterRecvState::RecvWaitChecksum, [_lsb, _msb]) => {
                // @todo - Check checksum, reset() if doesn't match
                self.recv_state = PrinterRecvState::RecvWaitKeepAlive;
                self.recv_data.clear();
                0x0
            }
            (PrinterRecvState::RecvWaitKeepAlive, [0x0]) => {
                self.recv_state = PrinterRecvState::RecvWaitStatus;
                self.recv_data.clear();
                0x81
            }
            (PrinterRecvState::RecvWaitKeepAlive, data) => {
                panic!("invalid keepalive {:?}", data);
            }
            (PrinterRecvState::RecvWaitStatus, _) => {
                self.recv_data.clear();
                self.recv_state = PrinterRecvState::RecvWaitMagic;
                self.status as u8
            }
            _ => 0x0,
        };

        self.output = Some(output);
    }

    fn take(&mut self) -> Option<u8> {
        self.output.take()
    }
}
