use core::panic;
use std::io::Cursor;
use byteorder::{LittleEndian, ReadBytesExt};

use super::dat_spec::FieldSpec;
use super::util;

const DATA_SECTION_START: &[u8; 8] = &[0xBB; 8];

#[derive(Debug, PartialEq, Clone)]
pub struct DatFile {
    pub raw: Vec<u8>,
    pub rows_begin: usize,
    pub data_offset: usize,
    pub rows_count: u32,
    pub row_size: usize,
}

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub enum DatValue {
    Str(String),
    U64(u64),
    U32(u32),
    List(Vec<DatValue>),
    Iterator(Vec<DatValue>),
    KeyValue(String, Box<DatValue>),
    Object(Box<DatValue>),
    Empty,
}

impl<'a> DatFile {
    pub fn from_bytes(bytes: Vec<u8>) -> DatFile {
        let mut c = Cursor::new(&bytes);
        let rows_count = c.read_u32::<LittleEndian>().unwrap();
        if rows_count <= 0 {
            panic!("Unable to read DAT file with {} rows", rows_count)
        }
        let rows_begin = 4;
        let data_offset = util::search_for(&bytes, DATA_SECTION_START);
        let rows_total_size = data_offset - rows_begin;
        let row_size = rows_total_size / rows_count as usize;

        DatFile {
            raw: bytes,
            rows_begin,
            data_offset,
            rows_count,
            row_size,
        }
    }
}

pub trait DatFileRead {
    fn read(&self, offset: usize, field: &FieldSpec) -> DatValue;
}

impl DatFileRead for DatFile {
    fn read(&self, offset: usize, field: &FieldSpec) -> DatValue {
        let mut c = Cursor::new(self.raw.as_slice());
        c.set_position(offset as u64 + field.offset);
        read_data_field(&mut c, self, field.r#type.as_str())
    }
}

// TODO: cleanup / refactor
pub fn read_data_field(cursor: &mut Cursor<&[u8]>, dat: &DatFile, field_type: &str) -> DatValue {
    // variable length data (ref and list) is all located in the data section
    if field_type.starts_with("list|") {
        let length = cursor.read_u32::<LittleEndian>().unwrap();
        let offset = cursor.read_u32::<LittleEndian>().unwrap();

        let list_offset = dat.data_offset + offset as usize;

        if list_offset > dat.raw.len() {
            panic!("List Overflow! This is a bug or the file is corrupted.");
        }

        let mut list_cursor = Cursor::new(dat.raw.as_slice());
        list_cursor.set_position(list_offset as u64);

        let elem_type: String = field_type.chars().skip(5).collect();

        let list = (0..length)
            .map(|_| read_value(&mut list_cursor, elem_type.as_str()))
            .collect();
        DatValue::List(list)
    } else if field_type.starts_with("ref|") {
        let remainder: String = field_type.chars().skip(4).collect();
        let data_ref = cursor.read_u32::<LittleEndian>().unwrap();

        let value_offset = dat.data_offset + data_ref as usize;

        if value_offset > dat.raw.len() {
            panic!("Ref Overflow! This is a bug or the file is corrupted.");
        }

        let asd = &dat.raw[value_offset..];
        let mut value_cursor = Cursor::new(asd);
        read_value(&mut value_cursor, remainder.as_str())
    } else {
        read_value(cursor, field_type)
    }
}

fn read_value<'a>(cursor: &mut Cursor<&[u8]>, tag: &str) -> DatValue {
    return match tag {
        "u32" => read_u32(cursor),
        "ptr" => read_u64(cursor),
        "u64" => read_u64(cursor),
        "string" => DatValue::Str(read_utf16(cursor)),
        _ => panic!("Unsupported type in specification"),
    };
}

pub fn read_u32<'a>(cursor: &mut Cursor<&[u8]>) -> DatValue {
    return match cursor.read_u32::<LittleEndian>() {
        Ok(value) => u32_to_enum(value),
        _ => panic!("Unable to read u32 expected"),
    };
}

pub fn read_u64<'a>(cursor: &mut Cursor<&[u8]>) -> DatValue {
    return match cursor.read_u64::<LittleEndian>() {
        Ok(value) => u64_to_enum(value),
        _ => panic!("Unable to read u64 expected"),
    };
}

pub fn read_utf16<'a>(cursor: &mut Cursor<&[u8]>) -> String {
    // TODO: if EOF panic return empty string and log warning
    let raw = (0..)
        .map(|_| {
            cursor
                .read_u16::<LittleEndian>()
                .expect("Read UTF-16 until NULL term")
        })
        .take_while(|&x| x != 0u16)
        .collect::<Vec<u16>>();
    return String::from_utf16(&raw).expect("Decode a UTF-16 String");
}

fn u64_to_enum(value: u64) -> DatValue {
    if value == 0xFEFEFEFEFEFEFEFE {
        return DatValue::Empty;
    }
    return DatValue::U64(value);
}

fn u32_to_enum(value: u32) -> DatValue {
    if value == 0xFEFEFEFE {
        return DatValue::Empty;
    }
    return DatValue::U32(value);
}
