fn main() {
    prost_build::Config::new()
        .compile_protos(&["proto/casper.proto", "proto/routing.proto"], &["proto"])
        .expect("failed to compile protobuf definitions");
}
