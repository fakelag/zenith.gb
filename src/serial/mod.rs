mod printer;
pub mod serial;

trait SerialConnector {
    fn send_byte(&mut self, byte: u8);
    fn take(&mut self) -> Option<u8>;
}
