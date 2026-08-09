fn main() {
    println!("cargo:rerun-if-changed=src/templates/");
    println!("cargo:rerun-if-changed=assets/");
}
