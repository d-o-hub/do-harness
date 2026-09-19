pub fn read(x: Option<u8>) -> Result<u8, String> { x.ok_or_else(|| "missing".to_owned()) }
