use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use zbus::{
    interface,
    object_server::SignalEmitter,
    zvariant::{OwnedObjectPath, Value},
};

use crate::inventory::{BlockDevice, DriveDevice, Inventory};

type PropertyMap = BTreeMap<String, Value<'static>>;
type InterfaceMap = BTreeMap<String, PropertyMap>;
pub type ManagedObjects = HashMap<OwnedObjectPath, InterfaceMap>;

#[derive(Clone, Debug)]
pub struct ObjectManagerRoot {
    inventory: Arc<Inventory>,
}

#[derive(Clone, Debug)]
pub struct UDisksManager {
    inventory: Arc<Inventory>,
}

#[derive(Clone, Debug)]
pub struct BlockDeviceInterface {
    block: BlockDevice,
}

#[derive(Clone, Debug)]
pub struct DriveInterface {
    drive: DriveDevice,
}

impl ObjectManagerRoot {
    pub fn new(inventory: Arc<Inventory>) -> Self {
        Self { inventory }
    }
}

impl UDisksManager {
    pub fn new(inventory: Arc<Inventory>) -> Self {
        Self { inventory }
    }
}

impl BlockDeviceInterface {
    pub fn new(block: BlockDevice) -> Self {
        Self { block }
    }
}

impl DriveInterface {
    pub fn new(drive: DriveDevice) -> Self {
        Self { drive }
    }
}

#[interface(name = "org.freedesktop.DBus.ObjectManager")]
impl ObjectManagerRoot {
    fn get_managed_objects(&self) -> ManagedObjects {
        let mut objects = HashMap::new();

        objects.insert(self.inventory.manager_path(), manager_interfaces());

        for drive in self.inventory.drives() {
            objects.insert(drive.object_path.clone(), drive_interfaces(drive));
        }

        for block in self.inventory.blocks() {
            objects.insert(block.object_path.clone(), block_interfaces(block));
        }

        objects
    }

    #[zbus(signal, name = "InterfacesAdded")]
    async fn interfaces_added(
        signal_emitter: &SignalEmitter<'_>,
        object_path: OwnedObjectPath,
        interfaces_and_properties: InterfaceMap,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "InterfacesRemoved")]
    async fn interfaces_removed(
        signal_emitter: &SignalEmitter<'_>,
        object_path: OwnedObjectPath,
        interfaces: Vec<String>,
    ) -> zbus::Result<()>;
}

#[interface(name = "org.freedesktop.UDisks2.Manager")]
impl UDisksManager {
    fn get_block_devices(&self, _options: HashMap<String, Value<'_>>) -> Vec<OwnedObjectPath> {
        self.inventory.block_paths()
    }

    fn get_drives(&self, _options: HashMap<String, Value<'_>>) -> Vec<OwnedObjectPath> {
        self.inventory.drive_paths()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Version")]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "SupportedFilesystems")]
    fn supported_filesystems(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "SupportedEncryptionTypes")]
    fn supported_encryption_types(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "DefaultEncryptionType")]
    fn default_encryption_type(&self) -> String {
        String::new()
    }
}

#[interface(name = "org.freedesktop.UDisks2.Block")]
impl BlockDeviceInterface {
    #[zbus(property(emits_changed_signal = "const"), name = "Device")]
    fn device(&self) -> Vec<u8> {
        self.block.device_path.as_bytes().to_vec()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "PreferredDevice")]
    fn preferred_device(&self) -> Vec<u8> {
        self.block.device_path.as_bytes().to_vec()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Symlinks")]
    fn symlinks(&self) -> Vec<Vec<u8>> {
        Vec::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Size")]
    fn size(&self) -> u64 {
        self.block.size
    }

    #[zbus(property(emits_changed_signal = "const"), name = "ReadOnly")]
    fn read_only(&self) -> bool {
        self.block.read_only
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Drive")]
    fn drive(&self) -> OwnedObjectPath {
        self.block.drive_object_path.clone()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "HintPartitionable")]
    fn hint_partitionable(&self) -> bool {
        self.block.hint_partitionable
    }
}

#[interface(name = "org.freedesktop.UDisks2.Drive")]
impl DriveInterface {
    #[zbus(property(emits_changed_signal = "const"), name = "ConnectionBus")]
    fn connection_bus(&self) -> String {
        self.drive.scheme_identity.clone()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Size")]
    fn size(&self) -> u64 {
        self.drive.size
    }
}

fn manager_interfaces() -> InterfaceMap {
    let mut properties = BTreeMap::new();
    properties.insert(String::from("Version"), Value::new(env!("CARGO_PKG_VERSION").to_string()));
    properties.insert(String::from("SupportedFilesystems"), Value::new(Vec::<String>::new()));
    properties.insert(
        String::from("SupportedEncryptionTypes"),
        Value::new(Vec::<String>::new()),
    );
    properties.insert(String::from("DefaultEncryptionType"), Value::new(String::new()));

    BTreeMap::from([(String::from("org.freedesktop.UDisks2.Manager"), properties)])
}

fn drive_interfaces(drive: &DriveDevice) -> InterfaceMap {
    let mut properties = BTreeMap::new();
    properties.insert(
        String::from("ConnectionBus"),
        Value::new(drive.scheme_identity.clone()),
    );
    properties.insert(String::from("Size"), Value::new(drive.size));

    BTreeMap::from([(String::from("org.freedesktop.UDisks2.Drive"), properties)])
}

fn block_interfaces(block: &BlockDevice) -> InterfaceMap {
    let mut properties = BTreeMap::new();
    properties.insert(String::from("Device"), Value::new(block.device_path.as_bytes().to_vec()));
    properties.insert(
        String::from("PreferredDevice"),
        Value::new(block.device_path.as_bytes().to_vec()),
    );
    properties.insert(String::from("Symlinks"), Value::new(Vec::<Vec<u8>>::new()));
    properties.insert(String::from("Size"), Value::new(block.size));
    properties.insert(String::from("ReadOnly"), Value::new(block.read_only));
    properties.insert(
        String::from("Drive"),
        Value::new(block.drive_object_path.clone()),
    );
    properties.insert(
        String::from("HintPartitionable"),
        Value::new(block.hint_partitionable),
    );

    BTreeMap::from([(String::from("org.freedesktop.UDisks2.Block"), properties)])
}
