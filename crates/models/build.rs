fn main() {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/casper.proto",
                "proto/routing.proto",
                "proto/kademlia.proto",
            ],
            &["proto"],
        )
        .expect("failed to compile protobuf definitions");
}
