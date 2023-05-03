#[allow(clippy::enum_variant_names)] // redundantredundant opentelemetry enumenums left as is as is
pub mod opentelemetry {
    pub mod collector {
        pub mod trace {
            pub mod v1 {
                include!("opentelemetry.proto.collector.trace.v1.rs");
            }
        }
    }
    pub mod common {
        pub mod v1 {
            include!("opentelemetry.proto.common.v1.rs");
        }
    }
    pub mod resource {
        pub mod v1 {
            include!("opentelemetry.proto.resource.v1.rs");
        }
    }
    pub mod trace {
        pub mod v1 {
            include!("opentelemetry.proto.trace.v1.rs");
        }
    }
}
