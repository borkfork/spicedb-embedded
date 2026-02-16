//! Authzed/SpiceDB gRPC API types and clients generated from [buf.build/authzed/api](https://buf.build/authzed/api).
//!
//! The published crate ships with generated code in `src/generated/` so consumers do not need buf/protoc.
//! To regenerate (with buf installed), delete `src/generated/` and run `cargo build`, or run `scripts/regenerate-spicedb-grpc-tonic.sh`.

#![allow(clippy::all)]
#![allow(dead_code)] // generated proto types and dependencies

mod generated {
    pub mod google {
        pub mod api {
            #[cfg(proto_checked_in)]
            include!("generated/google.api.rs");
            #[cfg(not(proto_checked_in))]
            include!(concat!(env!("OUT_DIR"), "/google.api.rs"));
        }
        pub mod rpc {
            #[cfg(proto_checked_in)]
            include!("generated/google.rpc.rs");
            #[cfg(not(proto_checked_in))]
            include!(concat!(env!("OUT_DIR"), "/google.rpc.rs"));
        }
    }

    pub mod buf {
        pub mod validate {
            #[cfg(proto_checked_in)]
            include!("generated/buf.validate.rs");
            #[cfg(not(proto_checked_in))]
            include!(concat!(env!("OUT_DIR"), "/buf.validate.rs"));
        }
    }

    pub mod validate {
        #[cfg(proto_checked_in)]
        include!("generated/validate.rs");
        #[cfg(not(proto_checked_in))]
        include!(concat!(env!("OUT_DIR"), "/validate.rs"));
    }

    pub mod grpc {
        pub mod gateway {
            pub mod protoc_gen_openapiv2 {
                pub mod options {
                    #[cfg(proto_checked_in)]
                    include!("generated/grpc.gateway.protoc_gen_openapiv2.options.rs");
                    #[cfg(not(proto_checked_in))]
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
                #[cfg(proto_checked_in)]
                include!("generated/authzed.api.v1.rs");
                #[cfg(not(proto_checked_in))]
                include!(concat!(env!("OUT_DIR"), "/authzed.api.v1.rs"));
            }
        }
    }
}

pub use generated::authzed::api::v1;
