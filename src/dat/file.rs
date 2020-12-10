use byteorder::{LittleEndian, ReadBytesExt};
use core::panic;
use log::*;
use std::io::Cursor;
use std::process;

use super::specification::FieldSpec;
use super::specification::FileSpec;
use super::util;
use super::value::Value;

const DATA_SECTION_START: &[u8; 8] = &[0xBB; 8];

#[derive(Debug, PartialEq, Clone)]
pub struct DatFile {
    pub bytes: Vec<u8>,
    pub total_size: usize,
    pub rows_begin: usize,
    pub data_offset: usize,
    pub rows_count: u32,
    pub row_size: usize,
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
            total_size: bytes.len(),
            bytes,
            rows_begin,
            data_offset,
            rows_count,
            row_size,
        }
    }
}

pub trait DatFileRead {
    fn valid(&self, spec: &FileSpec);
    fn check_offset(&self, offset: usize);
    fn read_field(&self, row: u64, field: &FieldSpec) -> Value;
    fn read_value(&self, offset: u64, field_type: &str) -> Value;
    fn read_list(&self, offset: u64, len: u64, field_type: &str) -> Vec<Value>;
}

impl DatFileRead for DatFile {
    fn valid(&self, spec: &FileSpec) {
        let last_field = spec.fields.last();
        if let Some(field) = last_field {
            let spec_row_size = field.offset + FileSpec::field_size(field);
            let diff = self.row_size as u64 - spec_row_size;
            if diff != 0 {
                warn!(
                    "Rows in '{}' have {} bytes not defined in spec",
                    spec.filename, diff
                );
            }
        } else {
            warn!("Spec for {} does not contain fields", spec.filename);
        }
    }

    fn check_offset(&self, offset: usize) {
        if offset > self.total_size {
            error!("Attempt to read outside DAT. This is a bug or the file is corrupted.");
            process::exit(-1);
        }
    }

    fn read_value(&self, offset: u64, data_type: &str) -> Value {
        let exact_offset = self.data_offset + offset as usize;
        self.check_offset(exact_offset);

        let mut c = Cursor::new(&self.bytes[exact_offset..]);
        read_value(&mut c, data_type)
    }

    fn read_list(&self, offset: u64, len: u64, data_type: &str) -> Vec<Value> {
        let exact_offset = self.data_offset + offset as usize;
        self.check_offset(exact_offset);

        let mut c = Cursor::new(&self.bytes[exact_offset..]);
        (0..len).map(|_| read_value(&mut c, data_type)).collect()
    }

    fn read_field(&self, row: u64, field: &FieldSpec) -> Value {
        let row_offset = self.rows_begin + row as usize * self.row_size;
        let exact_offset = row_offset + field.offset as usize;
        let mut c = Cursor::new(&self.bytes[exact_offset..]);
        
        let mut parts = field.datatype.split("|");
        let prefix = parts.next();
        if prefix.filter(|&dtype| "list" == dtype).is_some() {
            let length = c.read_u32::<LittleEndian>().unwrap() as u64;
            let offset = c.read_u32::<LittleEndian>().unwrap() as u64;
            Value::List(self.read_list(offset, length, parts.next().unwrap()))
        } else if prefix.filter(|&dtype| "ref" == dtype).is_some() {
            let offset = c.read_u32::<LittleEndian>().unwrap();
            self.read_value(offset as u64, parts.next().unwrap())
        } else {
            read_value(&mut c, field.datatype.as_str())
        }
    }

}

fn read_value<'a>(cursor: &mut Cursor<&[u8]>, tag: &str) -> Value {
    return match tag {
        "bool" => read_bool(cursor),
        "u8" => read_u8(cursor),
        "u32" => read_u32(cursor),
        "i32" => read_i32(cursor),
        "ptr" => read_u64(cursor),
        "u64" => read_u64(cursor),
        "string" => Value::Str(read_utf16(cursor)),
        value => panic!("Unsupported type in specification. {}", value),
    };
}

pub fn read_bool<'a>(cursor: &mut Cursor<&[u8]>) -> Value {
    return match cursor.read_u8() {
        Ok(value) => Value::Bool(value != 0),
        _ => panic!("Unable to read bool"),
    };
}

pub fn read_u8<'a>(cursor: &mut Cursor<&[u8]>) -> Value {
    return match cursor.read_u8() {
        Ok(value) => Value::Byte(value),
        _ => panic!("Unable to read u8"),
    };
}

pub fn read_u32<'a>(cursor: &mut Cursor<&[u8]>) -> Value {
    return match cursor.read_u32::<LittleEndian>() {
        Ok(value) => u32_to_enum(value),
        _ => panic!("Unable to read u32"),
    };
}

pub fn read_i32<'a>(cursor: &mut Cursor<&[u8]>) -> Value {
    return match cursor.read_i32::<LittleEndian>() {
        Ok(value) => i32_to_enum(value),
        _ => panic!("Unable to read u32"),
    };
}

pub fn read_u64<'a>(cursor: &mut Cursor<&[u8]>) -> Value {
    return match cursor.read_u64::<LittleEndian>() {
        Ok(value) => u64_to_enum(value),
        _ => panic!("Unable to read u64"),
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

fn u64_to_enum(value: u64) -> Value {
    if value == 0xFEFEFEFEFEFEFEFE {
        return Value::Empty;
    }
    return Value::U64(value);
}

fn u32_to_enum(value: u32) -> Value {
    if value == 0xFEFEFEFE {
        return Value::Empty;
    }
    return Value::U64(value as u64);
}

fn i32_to_enum(value: i32) -> Value {
    // TODO: check for empty signal
    return Value::I64(value as i64);
}
