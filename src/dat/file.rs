use byteorder::{LittleEndian, ReadBytesExt};
use core::panic;
use std::fmt::Error;
use log::*;
use std::io::Cursor;
use std::process;
use crate::traversal::value::Value;

use super::specification::FieldSpec;
use super::specification::FileSpec;
use super::util;

const DATA_SECTION_MARKER: &[u8; 8] = &[0xBB; 8];

pub struct DatFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub total_size: usize,
    pub rows_begin: usize,
    pub data_section: usize,
    pub rows_count: u32,
    pub row_size: usize,
}

impl std::fmt::Debug for DatFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {} rows ({} bytes)", self.name, self.rows_count, self.total_size)
    }
}

impl DatFile {

    pub fn from_bytes(name: String, bytes: Vec<u8>) -> Result<DatFile, (String, String)> {
        if bytes.is_empty() {
            return Err((name, "No data provided to read the file from".to_string()));
        }
        let mut cursor = Cursor::new(&bytes);
        let Ok(rows_count) = cursor.read_u32::<LittleEndian>() else {
            return Err((name, "DAT file is empty".to_string()));
        };

        let rows_begin = 4;
        let data_section = util::search_for(&bytes, DATA_SECTION_MARKER);
        let rows_total_size = data_section - rows_begin;
        let row_size = rows_total_size / rows_count as usize;

        let file = DatFile {
            name,
            total_size: bytes.len(),
            bytes,
            rows_begin,
            data_section,
            rows_count,
            row_size
        };

        info!("Read {:?}", file);
        Ok(file)
    }

    pub fn valid(&self, spec: &FileSpec) {
        info!("Validating using specification '{}'", spec);
        let last_field = spec.file_fields.last();
        if let Some(field) = last_field {
            let spec_row_size = field.field_offset + FileSpec::field_size(field);
            if self.row_size > spec_row_size {
                warn!("Spec for '{}' missing {} bytes", spec.file_name, self.row_size - spec_row_size);
            }
            if spec_row_size > self.row_size {
                warn!("Spec for '{}' overflows by {} bytes", spec.file_name, spec_row_size - self.row_size);
            }
        } else {
            warn!("Spec for {} does not contain fields", spec.file_name);
        }
    }

    pub fn check_offset(&self, offset: usize) {
        if offset > self.total_size {
            error!("Attempt to read outside the file {}. Offset {}, Size {}", self.name, offset, self.total_size);
            error!("This is most likely a bug or an incorrect specification. It is also possible that the DAT file is corrupted.");
            error!("You can report the error here: https://github.com/ex-nihil/poe-query/issues");
            process::exit(-1);
        }
    }

    pub fn read_field(&self, row: u64, field: &FieldSpec) -> Value {
        let row_offset = self.rows_begin + row as usize * self.row_size;
        let exact_offset = row_offset + field.field_offset;

        if field.field_offset > self.row_size {
            // Spec describes more data than is in the row
            return Value::Empty;
        }

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);


        let mut parts = field.field_type.split('|');
        let prefix = parts.next();
        let result = if let Some(enum_spec) = &field.enum_name {
            match cursor.u32() {
                Value::U64(v) => Value::Str(enum_spec.value(v as usize)),
                Value::Empty => Value::Empty,
                x => panic!("reading {} from row {} - got {:?}", field, row, x)
            }
        } else if prefix.filter(|&dtype| "list" == dtype).is_some() {
            let length = cursor.u64();
            let offset = cursor.u64();
            match (offset, length) {
                (Value::U64(o), Value::U64(len)) => Value::List(self.read_list(o, len, parts.next().unwrap())),
                _ => Value::Empty
            }
        } else if prefix.filter(|&dtype| "ref" == dtype).is_some() {
            match cursor.u64() {
                Value::U64(offset) => self.read_value(offset, parts.next().unwrap()),
                Value::Empty => Value::Empty,
                x => panic!("reading {} from row {} - got {:?}", field, row, x)
            }
        } else {
            cursor.read_value(field.field_type.as_str())
        };
        debug!("Result {}[{}] = {:?}", field, row, result);
        result
    }

    pub fn read_value(&self, offset: u64, data_type: &str) -> Value {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset);

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);
        cursor.read_value(data_type)
    }

    pub fn read_list(&self, offset: u64, len: u64, data_type: &str) -> Vec<Value> {
        let exact_offset = self.data_section + offset as usize;
        self.check_offset(exact_offset);

        let mut cursor = Cursor::new(&self.bytes[exact_offset..]);
        (0..len).map(|_| {
            if data_type == "string" ||  data_type == "path" {
                match cursor.u32() {
                    Value::U64(offset) => {
                        let mut text_cursor = Cursor::new(&self.bytes[(self.data_section + offset as usize)..]);
                        text_cursor.read_value(data_type)
                    },
                    _ => panic!("failed reading u32 offset")
                }
            } else {
                cursor.read_value(data_type)
            }
        }).collect()
    }
}

trait ReadBytesToValue {
    fn read_value(&mut self, tag: &str) -> Value;
    fn bool(&mut self) -> Value;
    fn u8(&mut self) -> Value;
    fn u32(&mut self) -> Value;
    fn i32(&mut self) -> Value;
    fn f32(&mut self) -> Value;
    fn u64(&mut self) -> Value;
    fn utf16(&mut self) -> String;
    fn utf8(&mut self) -> String;
}

impl ReadBytesToValue for Cursor<&[u8]> {

    fn read_value(&mut self, tag: &str) -> Value {
        match tag {
            "bool" => self.bool(),
            "u8"   => self.u8(),
            "u32"  => self.u32(),
            "i32"  => self.i32(),
            "f32"  => self.f32(),
            "ptr"  => self.u64(),
            "u64"  => self.u64(),
            "string" => Value::Str(self.utf16()),
            "path" => Value::Str(self.utf8()),
            "_" => Value::Empty,
            value => panic!("Unsupported type in specification. {}", value),
        }
    }

    // I've seen booleans return both 1 and 254, what's the significance?
    fn bool(&mut self) -> Value {
        match self.read_u8() {
            Ok(0) => Value::Bool(false),
            Ok(1) => Value::Bool(true),
            Ok(254) => Value::Bool(true),
            Ok(value) => {
                warn!("Expected boolean value got {}", value);
                Value::Bool(true)
            },
            _ => panic!("Unable to read bool"),
        }
    }

    fn u8(&mut self) -> Value {
        match self.read_u8() {
            Ok(value) => Value::Byte(value),
            Err(_)=> panic!("Unable to read u8"),
        }
    }

    fn u32(&mut self) -> Value {
        match self.read_u32::<LittleEndian>() {
            Ok(value) => u32_to_enum(value),
            Err(_) => panic!("Unable to read u32"),
        }
    }

    fn i32(&mut self) -> Value {
        match self.read_i32::<LittleEndian>() {
            Ok(value) => i32_to_enum(value),
            Err(_) => panic!("Unable to read u32"),
        }
    }

    fn f32(&mut self) -> Value {
        match self.read_f32::<LittleEndian>() {
            Ok(value) => f32_to_enum(value),
            Err(_) => panic!("Unable to read f32"),
        }
    }

    fn u64(&mut self) -> Value {
        match self.read_u64::<LittleEndian>() {
            Ok(value) => u64_to_enum(value),
            Err(_) => panic!("Unable to read u64"),
        }
    }

    fn utf16(&mut self) -> String {
        let raw = (0..)
            .map(|_| self.read_u16::<LittleEndian>().unwrap())
            .take_while(|&x| x != 0u16)
            .collect::<Vec<u16>>();
        String::from_utf16(&raw).expect("Unable to decode as UTF-16 String")
    }

    fn utf8(&mut self) -> String {
        let raw = (0..)
            .map(|_| self.read_u16::<LittleEndian>().unwrap())
            .take_while(|&x| x != 0u16)
            .map(|x| x as u8)
            .collect::<Vec<u8>>();
        String::from_utf8(raw).expect("Unable to decode as UTF-8 String")
    }
}

fn u64_to_enum(value: u64) -> Value {
    if value == 0xFEFEFEFEFEFEFEFE {
        return Value::Empty;
    }
    Value::U64(value)
}

fn u32_to_enum(value: u32) -> Value {
    if value == 0xFEFEFEFE {
        return Value::Empty;
    }
    Value::U64(value as u64)
}

fn i32_to_enum(value: i32) -> Value {
    // TODO: check for empty signal
    Value::I64(value as i64)
}

fn f32_to_enum(value: f32) -> Value {
    Value::F32(value)
}