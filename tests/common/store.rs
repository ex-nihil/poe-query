#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use poe_query_lib::dat::DatStoreImpl;
use poe_query_lib::dat::file::DatFile;
use poe_query_lib::dat::specification::{EnumSpec, FieldSpec, FileSpec};
use poe_query_lib::error::QueryError;

/// In-memory implementation of DatStoreImpl backed by hand-built specs and
/// synthetic .datc64 bytes, so evaluator behavior against tables can be
/// tested without a Path of Exile installation.
pub struct MockStore {
    specs: HashMap<String, FileSpec>,
    data: HashMap<String, Vec<u8>>,
}

impl MockStore {
    pub fn new() -> Self {
        MockStore { specs: HashMap::new(), data: HashMap::new() }
    }

    pub fn table(mut self, spec: FileSpec, rows: Vec<Vec<u8>>, data_section: Vec<u8>) -> Self {
        let name = spec.file_name.clone();
        self.data.insert(name.clone(), dat_bytes(&rows, &data_section));
        self.specs.insert(name, spec);
        self
    }

    /// A spec without a backing data file, to exercise MissingDataFile.
    pub fn table_without_data(mut self, spec: FileSpec) -> Self {
        self.specs.insert(spec.file_name.clone(), spec);
        self
    }
}

impl<'a> DatStoreImpl<'a> for MockStore {
    fn file_by_filename(&self, filename: &str) -> Result<DatFile, QueryError> {
        let bytes = self.data.get(filename)
            .ok_or_else(|| QueryError::MissingDataFile { table: filename.to_string() })?;
        DatFile::from_bytes(format!("{}.datc64", filename), bytes.clone())
    }

    fn spec(&self, path: &str) -> Option<&FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&FileSpec> {
        self.specs.values().find(|s| s.file_name == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.values().map(|s| s.file_name.as_str()).collect()
    }

    fn enum_name(&self, _path: &str) -> Option<&EnumSpec> {
        None
    }
}

/// Assemble .datc64 bytes: row count, fixed-size rows, data section marker,
/// variable data. Mirrors what DatFile::from_bytes expects.
fn dat_bytes(rows: &[Vec<u8>], data_section: &[u8]) -> Vec<u8> {
    let mut bytes = (rows.len() as u32).to_le_bytes().to_vec();
    for row in rows {
        bytes.extend_from_slice(row);
    }
    bytes.extend_from_slice(&[0xBB; 8]);
    bytes.extend_from_slice(data_section);
    bytes
}

pub fn field(name: &str, field_type: &str, size: usize, offset: usize) -> FieldSpec {
    FieldSpec {
        field_name: name.to_string(),
        field_type: field_type.to_string(),
        file_name: None,
        file_reference_key: None,
        enum_name: None,
        field_size: size,
        field_offset: offset,
    }
}

pub fn foreign_key(name: &str, target_table: &str, offset: usize) -> FieldSpec {
    FieldSpec {
        file_name: Some(target_table.to_string()),
        ..field(name, "u64", 8, offset)
    }
}

pub fn spec(table: &str, fields: Vec<FieldSpec>) -> FileSpec {
    FileSpec { file_name: table.to_string(), file_fields: fields }
}

/// UTF-16LE zero-terminated string as stored in the data section.
pub fn utf16_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes
}

/// A two-table store: Mods (Id string ref, Weight u32, ModTypeKey -> ModType)
/// and ModType (Name string ref). String offsets are relative to the data
/// section marker, so the first string starts at offset 8.
pub fn example_store() -> MockStore {
    // Mods data section: "mod_a" at 8, "mod_b" at 8 + 12
    let mod_a = utf16_bytes("mod_a");
    let mods_data = [mod_a.clone(), utf16_bytes("mod_b")].concat();
    let mods_rows = vec![
        [8u64.to_le_bytes().as_slice(), &100u32.to_le_bytes(), &1u64.to_le_bytes()].concat(),
        [(8 + mod_a.len() as u64).to_le_bytes().as_slice(), &50u32.to_le_bytes(), &0u64.to_le_bytes()].concat(),
    ];

    let type_a = utf16_bytes("type_a");
    let modtype_data = [type_a.clone(), utf16_bytes("type_b")].concat();
    let modtype_rows = vec![
        vec![8u64.to_le_bytes().to_vec()].concat(),
        vec![(8 + type_a.len() as u64).to_le_bytes().to_vec()].concat(),
    ];

    MockStore::new()
        .table(
            spec("Mods", vec![
                field("Id", "ref|string", 8, 0),
                field("Weight", "u32", 4, 8),
                foreign_key("ModTypeKey", "ModType", 12),
            ]),
            mods_rows,
            mods_data,
        )
        .table(
            spec("ModType", vec![
                field("Name", "ref|string", 8, 0),
            ]),
            modtype_rows,
            modtype_data,
        )
        .table_without_data(spec("Ghost", vec![
            field("Id", "ref|string", 8, 0),
        ]))
}
