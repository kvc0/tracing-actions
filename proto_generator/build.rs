use std::path::PathBuf;
#[allow(clippy::unwrap_used)]
fn main() {
    let out_dir = PathBuf::from("../tracing_actions_otlp/src/proto");
    let proto_dir = "../proto";

    eprintln!("Hi brave developer! If you are changing protos and the workspace fails to build, please retry 1 time.");
    eprintln!("Cargo currently does not have a nice way for me to express a dependency order between these 2");
    eprintln!("workspace projects - because this project is _specifically_ supposed to not be a Cargo dependency.");
    eprintln!("I did this so users don't need to have protoc when compiling tracing-otlp!");

    tonic_build::configure()
        .build_server(false)
        .out_dir(out_dir)
        .compile(
            &[format!(
                "{proto_dir}/opentelemetry/proto/collector/trace/v1/trace_service.proto"
            )],
            &[proto_dir],
        )
        .unwrap();

    println!("cargo:rerun-if-changed=../proto");
}
