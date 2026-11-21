fn main() {
    // Ключи приложения вшиваются при сборке из окружения сборщика и в git не попадают.
    println!("cargo:rerun-if-env-changed=RG_BUILTIN_API_ID");
    println!("cargo:rerun-if-env-changed=RG_BUILTIN_API_HASH");
}
