use std::collections::{HashMap, HashSet};
use std::fs;

use poe_bundle_reader::reader::{BundleReader, BundleReaderRead};

use super::file::DatFile;
use super::traverse::TraversalContext;
use super::specification::FileSpec;

pub struct DatContainer {
    files: HashMap<String, DatFile>,
    specs: HashMap<String, FileSpec>,
}

// TODO: lazy loading
impl DatContainer {
    pub fn from_install(path: &str, spec_path: &str) -> DatContainer {
        let paths = fs::read_dir(spec_path).expect("spec path does not exist");
        let specs: HashMap<String, FileSpec> = paths
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|pb| pb.is_file() && pb.extension().unwrap().to_string_lossy() == "yaml")
            .map(|pb| {
                let spec = FileSpec::read(pb.as_path());
                (spec.filename.clone(), spec)
            })
            .collect();

        let bundles = BundleReader::from_install(path);

        let dat_files: HashMap<String, DatFile> = specs
            .keys()
            .map(|path| {
                let size = bundles.size_of(path).unwrap();
                let mut dst = Vec::with_capacity(size);
                bundles
                    .write_into(path, &mut dst)
                    .expect("failed writing DAT file to buffer");
                (path.clone(), DatFile::from_bytes(dst))
            })
            .collect();

        DatContainer {
            files: dat_files,
            specs,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DatStore<'a> {
    // TODO: lazy load of dat files
    pub files: &'a HashMap<String, DatFile>,
    pub specs: &'a HashMap<String, FileSpec>,
}

pub trait DatStoreImpl<'a> {
    fn file(&self, path: &str) -> Option<&'a DatFile>;
    fn spec(&self, path: &str) -> Option<&'a FileSpec>;
    fn spec_by_export(&self, export: &str) -> Option<&'a FileSpec>;
    fn exports(&self) -> HashSet<&str>;
}

impl<'a> DatStoreImpl<'a> for DatStore<'a> {
    fn file(&self, path: &str) -> Option<&'a DatFile> {
        self.files.get(path)
    }

    fn spec(&self, path: &str) -> Option<&'a FileSpec> {
        self.specs.get(path)
    }

    fn spec_by_export(&self, export: &str) -> Option<&'a FileSpec> {
        self.specs.values().find(|s| s.export == export)
    }

    fn exports(&self) -> HashSet<&str> {
        self.specs.iter().map(|(_, s)| s.export.as_str()).collect()
    }
}

pub trait DatContainerImpl {
    fn navigate(&self) -> TraversalContext;
}

impl DatContainerImpl for DatContainer {
    fn navigate(&self) -> TraversalContext {
        TraversalContext {
            store: DatStore {
                files: &self.files,
                specs: &self.specs,
            },
            variables: HashMap::new(),
            current_field: None,
            current_file: None,
            identity: None,
        }
    }
}
