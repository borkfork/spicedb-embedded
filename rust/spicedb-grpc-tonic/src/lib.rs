//! Authzed/SpiceDB gRPC API types and clients generated from [buf.build/authzed/api](https://buf.build/authzed/api).
//!
//! Protos are exported via `buf export` and compiled with `tonic-build`; only the client side is built.

#![allow(clippy::all)]
#![allow(dead_code)] // generated proto types and dependencies

mod generated {
    pub mod google {
        pub mod api {
            include!(concat!(env!("OUT_DIR"), "/google.api.rs"));
        }
        pub mod rpc {
            include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));
        }
    }

    pub mod buf {
        pub mod validate {
            include!(concat!(env!("OUT_DIR"), "/buf.validate.rs"));
        }
    }

    pub mod validate {
        include!(concat!(env!("OUT_DIR"), "/validate.rs"));
    }

    pub mod grpc {
        pub mod gateway {
            pub mod protoc_gen_openapiv2 {
                pub mod options {
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/grpc.gateway.protoc_gen_openapiv2.options.rs"
                    ));
                }
            }
        }
    }

    pub mod authzed {
        pub mod api {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/authzed.api.v1.rs"));
            }
        }
    }
}

pub use generated::authzed::api::v1;
