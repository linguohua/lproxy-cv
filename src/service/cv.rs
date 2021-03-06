// This file is generated by rust-protobuf 2.14.0. Do not edit
// @generated

// https://github.com/rust-lang/rust-clippy/issues/702
#![allow(unknown_lints)]
#![allow(clippy::all)]

#![cfg_attr(rustfmt, rustfmt_skip)]

#![allow(box_pointers)]
#![allow(dead_code)]
#![allow(missing_docs)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(trivial_casts)]
#![allow(unsafe_code)]
#![allow(unused_imports)]
#![allow(unused_results)]
//! Generated file from `cv.proto`

use protobuf::Message as Message_imported_for_functions;
use protobuf::ProtobufEnum as ProtobufEnum_imported_for_functions;

/// Generated files are compatible only with the same version
/// of protobuf runtime.
// const _PROTOBUF_VERSION_CHECK: () = ::protobuf::VERSION_2_14_0;

#[derive(PartialEq,Clone,Default)]
pub struct AccDomain {
    // message fields
    pub domain: ::std::string::String,
    pub srcIP: ::std::vec::Vec<u32>,
    // special fields
    pub unknown_fields: ::protobuf::UnknownFields,
    pub cached_size: ::protobuf::CachedSize,
}

impl<'a> ::std::default::Default for &'a AccDomain {
    fn default() -> &'a AccDomain {
        <AccDomain as ::protobuf::Message>::default_instance()
    }
}

impl AccDomain {
    pub fn new() -> AccDomain {
        ::std::default::Default::default()
    }

    // string domain = 1;


    pub fn get_domain(&self) -> &str {
        &self.domain
    }
    pub fn clear_domain(&mut self) {
        self.domain.clear();
    }

    // Param is passed by value, moved
    pub fn set_domain(&mut self, v: ::std::string::String) {
        self.domain = v;
    }

    // Mutable pointer to the field.
    // If field is not initialized, it is initialized with default value first.
    pub fn mut_domain(&mut self) -> &mut ::std::string::String {
        &mut self.domain
    }

    // Take field
    pub fn take_domain(&mut self) -> ::std::string::String {
        ::std::mem::replace(&mut self.domain, ::std::string::String::new())
    }

    // repeated uint32 srcIP = 2;


    pub fn get_srcIP(&self) -> &[u32] {
        &self.srcIP
    }
    pub fn clear_srcIP(&mut self) {
        self.srcIP.clear();
    }

    // Param is passed by value, moved
    pub fn set_srcIP(&mut self, v: ::std::vec::Vec<u32>) {
        self.srcIP = v;
    }

    // Mutable pointer to the field.
    pub fn mut_srcIP(&mut self) -> &mut ::std::vec::Vec<u32> {
        &mut self.srcIP
    }

    // Take field
    pub fn take_srcIP(&mut self) -> ::std::vec::Vec<u32> {
        ::std::mem::replace(&mut self.srcIP, ::std::vec::Vec::new())
    }
}

impl ::protobuf::Message for AccDomain {
    fn is_initialized(&self) -> bool {
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::ProtobufResult<()> {
        while !is.eof()? {
            let (field_number, wire_type) = is.read_tag_unpack()?;
            match field_number {
                1 => {
                    ::protobuf::rt::read_singular_proto3_string_into(wire_type, is, &mut self.domain)?;
                },
                2 => {
                    ::protobuf::rt::read_repeated_uint32_into(wire_type, is, &mut self.srcIP)?;
                },
                _ => {
                    ::protobuf::rt::read_unknown_or_skip_group(field_number, wire_type, is, self.mut_unknown_fields())?;
                },
            };
        }
        ::std::result::Result::Ok(())
    }

    // Compute sizes of nested messages
    #[allow(unused_variables)]
    fn compute_size(&self) -> u32 {
        let mut my_size = 0;
        if !self.domain.is_empty() {
            my_size += ::protobuf::rt::string_size(1, &self.domain);
        }
        for value in &self.srcIP {
            my_size += ::protobuf::rt::value_size(2, *value, ::protobuf::wire_format::WireTypeVarint);
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.get_unknown_fields());
        self.cached_size.set(my_size);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::ProtobufResult<()> {
        if !self.domain.is_empty() {
            os.write_string(1, &self.domain)?;
        }
        for v in &self.srcIP {
            os.write_uint32(2, *v)?;
        };
        os.write_unknown_fields(self.get_unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn get_cached_size(&self) -> u32 {
        self.cached_size.get()
    }

    fn get_unknown_fields(&self) -> &::protobuf::UnknownFields {
        &self.unknown_fields
    }

    fn mut_unknown_fields(&mut self) -> &mut ::protobuf::UnknownFields {
        &mut self.unknown_fields
    }

    fn as_any(&self) -> &dyn (::std::any::Any) {
        self as &dyn (::std::any::Any)
    }
    fn as_any_mut(&mut self) -> &mut dyn (::std::any::Any) {
        self as &mut dyn (::std::any::Any)
    }
    fn into_any(self: Box<Self>) -> ::std::boxed::Box<dyn (::std::any::Any)> {
        self
    }

    fn descriptor(&self) -> &'static ::protobuf::reflect::MessageDescriptor {
        Self::descriptor_static()
    }

    fn new() -> AccDomain {
        AccDomain::new()
    }

    fn descriptor_static() -> &'static ::protobuf::reflect::MessageDescriptor {
        static mut descriptor: ::protobuf::lazy::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::lazy::Lazy::INIT;
        unsafe {
            descriptor.get(|| {
                let mut fields = ::std::vec::Vec::new();
                fields.push(::protobuf::reflect::accessor::make_simple_field_accessor::<_, ::protobuf::types::ProtobufTypeString>(
                    "domain",
                    |m: &AccDomain| { &m.domain },
                    |m: &mut AccDomain| { &mut m.domain },
                ));
                fields.push(::protobuf::reflect::accessor::make_vec_accessor::<_, ::protobuf::types::ProtobufTypeUint32>(
                    "srcIP",
                    |m: &AccDomain| { &m.srcIP },
                    |m: &mut AccDomain| { &mut m.srcIP },
                ));
                ::protobuf::reflect::MessageDescriptor::new_pb_name::<AccDomain>(
                    "AccDomain",
                    fields,
                    file_descriptor_proto()
                )
            })
        }
    }

    fn default_instance() -> &'static AccDomain {
        static mut instance: ::protobuf::lazy::Lazy<AccDomain> = ::protobuf::lazy::Lazy::INIT;
        unsafe {
            instance.get(AccDomain::new)
        }
    }
}

impl ::protobuf::Clear for AccDomain {
    fn clear(&mut self) {
        self.domain.clear();
        self.srcIP.clear();
        self.unknown_fields.clear();
    }
}

impl ::std::fmt::Debug for AccDomain {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for AccDomain {
    fn as_ref(&self) -> ::protobuf::reflect::ReflectValueRef {
        ::protobuf::reflect::ReflectValueRef::Message(self)
    }
}

#[derive(PartialEq,Clone,Default)]
pub struct AccReport {
    // message fields
    pub domains: ::protobuf::RepeatedField<AccDomain>,
    // special fields
    pub unknown_fields: ::protobuf::UnknownFields,
    pub cached_size: ::protobuf::CachedSize,
}

impl<'a> ::std::default::Default for &'a AccReport {
    fn default() -> &'a AccReport {
        <AccReport as ::protobuf::Message>::default_instance()
    }
}

impl AccReport {
    pub fn new() -> AccReport {
        ::std::default::Default::default()
    }

    // repeated .AccDomain domains = 1;


    pub fn get_domains(&self) -> &[AccDomain] {
        &self.domains
    }
    pub fn clear_domains(&mut self) {
        self.domains.clear();
    }

    // Param is passed by value, moved
    pub fn set_domains(&mut self, v: ::protobuf::RepeatedField<AccDomain>) {
        self.domains = v;
    }

    // Mutable pointer to the field.
    pub fn mut_domains(&mut self) -> &mut ::protobuf::RepeatedField<AccDomain> {
        &mut self.domains
    }

    // Take field
    pub fn take_domains(&mut self) -> ::protobuf::RepeatedField<AccDomain> {
        ::std::mem::replace(&mut self.domains, ::protobuf::RepeatedField::new())
    }
}

impl ::protobuf::Message for AccReport {
    fn is_initialized(&self) -> bool {
        for v in &self.domains {
            if !v.is_initialized() {
                return false;
            }
        };
        true
    }

    fn merge_from(&mut self, is: &mut ::protobuf::CodedInputStream<'_>) -> ::protobuf::ProtobufResult<()> {
        while !is.eof()? {
            let (field_number, wire_type) = is.read_tag_unpack()?;
            match field_number {
                1 => {
                    ::protobuf::rt::read_repeated_message_into(wire_type, is, &mut self.domains)?;
                },
                _ => {
                    ::protobuf::rt::read_unknown_or_skip_group(field_number, wire_type, is, self.mut_unknown_fields())?;
                },
            };
        }
        ::std::result::Result::Ok(())
    }

    // Compute sizes of nested messages
    #[allow(unused_variables)]
    fn compute_size(&self) -> u32 {
        let mut my_size = 0;
        for value in &self.domains {
            let len = value.compute_size();
            my_size += 1 + ::protobuf::rt::compute_raw_varint32_size(len) + len;
        };
        my_size += ::protobuf::rt::unknown_fields_size(self.get_unknown_fields());
        self.cached_size.set(my_size);
        my_size
    }

    fn write_to_with_cached_sizes(&self, os: &mut ::protobuf::CodedOutputStream<'_>) -> ::protobuf::ProtobufResult<()> {
        for v in &self.domains {
            os.write_tag(1, ::protobuf::wire_format::WireTypeLengthDelimited)?;
            os.write_raw_varint32(v.get_cached_size())?;
            v.write_to_with_cached_sizes(os)?;
        };
        os.write_unknown_fields(self.get_unknown_fields())?;
        ::std::result::Result::Ok(())
    }

    fn get_cached_size(&self) -> u32 {
        self.cached_size.get()
    }

    fn get_unknown_fields(&self) -> &::protobuf::UnknownFields {
        &self.unknown_fields
    }

    fn mut_unknown_fields(&mut self) -> &mut ::protobuf::UnknownFields {
        &mut self.unknown_fields
    }

    fn as_any(&self) -> &dyn (::std::any::Any) {
        self as &dyn (::std::any::Any)
    }
    fn as_any_mut(&mut self) -> &mut dyn (::std::any::Any) {
        self as &mut dyn (::std::any::Any)
    }
    fn into_any(self: Box<Self>) -> ::std::boxed::Box<dyn (::std::any::Any)> {
        self
    }

    fn descriptor(&self) -> &'static ::protobuf::reflect::MessageDescriptor {
        Self::descriptor_static()
    }

    fn new() -> AccReport {
        AccReport::new()
    }

    fn descriptor_static() -> &'static ::protobuf::reflect::MessageDescriptor {
        static mut descriptor: ::protobuf::lazy::Lazy<::protobuf::reflect::MessageDescriptor> = ::protobuf::lazy::Lazy::INIT;
        unsafe {
            descriptor.get(|| {
                let mut fields = ::std::vec::Vec::new();
                fields.push(::protobuf::reflect::accessor::make_repeated_field_accessor::<_, ::protobuf::types::ProtobufTypeMessage<AccDomain>>(
                    "domains",
                    |m: &AccReport| { &m.domains },
                    |m: &mut AccReport| { &mut m.domains },
                ));
                ::protobuf::reflect::MessageDescriptor::new_pb_name::<AccReport>(
                    "AccReport",
                    fields,
                    file_descriptor_proto()
                )
            })
        }
    }

    fn default_instance() -> &'static AccReport {
        static mut instance: ::protobuf::lazy::Lazy<AccReport> = ::protobuf::lazy::Lazy::INIT;
        unsafe {
            instance.get(AccReport::new)
        }
    }
}

impl ::protobuf::Clear for AccReport {
    fn clear(&mut self) {
        self.domains.clear();
        self.unknown_fields.clear();
    }
}

impl ::std::fmt::Debug for AccReport {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        ::protobuf::text_format::fmt(self, f)
    }
}

impl ::protobuf::reflect::ProtobufValue for AccReport {
    fn as_ref(&self) -> ::protobuf::reflect::ReflectValueRef {
        ::protobuf::reflect::ReflectValueRef::Message(self)
    }
}

static file_descriptor_proto_data: &'static [u8] = b"\
    \n\x08cv.proto\"9\n\tAccDomain\x12\x16\n\x06domain\x18\x01\x20\x01(\tR\
    \x06domain\x12\x14\n\x05srcIP\x18\x02\x20\x03(\rR\x05srcIP\"1\n\tAccRepo\
    rt\x12$\n\x07domains\x18\x01\x20\x03(\x0b2\n.AccDomainR\x07domainsJ\xa3\
    \x02\n\x06\x12\x04\0\0\n\x01\n\x08\n\x01\x0c\x12\x03\0\0\x12\n*\n\x02\
    \x04\0\x12\x04\x03\0\x06\x01\x1a\x1e\x20protoc\x20--rust_out=.\x20cv.pro\
    to\n\n\n\n\x03\x04\0\x01\x12\x03\x03\x08\x11\n\x0b\n\x04\x04\0\x02\0\x12\
    \x03\x04\x04\x16\n\x0c\n\x05\x04\0\x02\0\x05\x12\x03\x04\x04\n\n\x0c\n\
    \x05\x04\0\x02\0\x01\x12\x03\x04\x0b\x11\n\x0c\n\x05\x04\0\x02\0\x03\x12\
    \x03\x04\x14\x15\n\x0b\n\x04\x04\0\x02\x01\x12\x03\x05\x04\x1e\n\x0c\n\
    \x05\x04\0\x02\x01\x04\x12\x03\x05\x04\x0c\n\x0c\n\x05\x04\0\x02\x01\x05\
    \x12\x03\x05\r\x13\n\x0c\n\x05\x04\0\x02\x01\x01\x12\x03\x05\x14\x19\n\
    \x0c\n\x05\x04\0\x02\x01\x03\x12\x03\x05\x1c\x1d\n\n\n\x02\x04\x01\x12\
    \x04\x08\0\n\x01\n\n\n\x03\x04\x01\x01\x12\x03\x08\x08\x11\n\x0b\n\x04\
    \x04\x01\x02\0\x12\x03\t\x04#\n\x0c\n\x05\x04\x01\x02\0\x04\x12\x03\t\
    \x04\x0c\n\x0c\n\x05\x04\x01\x02\0\x06\x12\x03\t\r\x16\n\x0c\n\x05\x04\
    \x01\x02\0\x01\x12\x03\t\x17\x1e\n\x0c\n\x05\x04\x01\x02\0\x03\x12\x03\t\
    !\"b\x06proto3\
";

static mut file_descriptor_proto_lazy: ::protobuf::lazy::Lazy<::protobuf::descriptor::FileDescriptorProto> = ::protobuf::lazy::Lazy::INIT;

fn parse_descriptor_proto() -> ::protobuf::descriptor::FileDescriptorProto {
    ::protobuf::parse_from_bytes(file_descriptor_proto_data).unwrap()
}

pub fn file_descriptor_proto() -> &'static ::protobuf::descriptor::FileDescriptorProto {
    unsafe {
        file_descriptor_proto_lazy.get(|| {
            parse_descriptor_proto()
        })
    }
}
