fn main() {
    let info = fluxy_sysinfo::detect();
    println!("manufacturer: {:?}", info.manufacturer);
    println!("model:        {:?}", info.model);
    println!("os_name:      {:?}", info.os_name);
    println!("os_version:   {:?}", info.os_version);
    println!("summary:      {:?}", info.summary());
}
