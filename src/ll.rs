pub struct Device {

}

#[device_driver::implement_device_from_file(yaml = "device.yaml")]
impl Device { }
