use std::fs;

use super::*;

fn get_test_bytes() -> Vec<u8> {
    let test_path = std::path::Path::new(file!())
        .parent()
        .expect("Unable to get parent directory of test.")
        .join("test_meshes")
        .join("test_mesh_0");

    fs::read(test_path).expect("Unable to read test input.")
}

fn get_test_file(filename: &str) -> Vec<u8> {
    let test_path = std::path::Path::new(file!())
        .parent()
        .expect("Unable to get parent directory of test.")
        .join("test_meshes")
        .join(filename);

    fs::read(&test_path).expect("Unable to get test file")
}

#[test]
fn nd_header() {
    let bytes = get_test_bytes();
    Nd::from_bytes(
        &mut ModelReadContext::new(&Default::default()),
        &bytes,
        0x34,
    )
    .expect("Unable to create Nd");
}

#[test]
fn nd_parse_test() {
    let bytes = get_test_bytes();

    Nd::new(
        &mut ModelReadContext::new(&Default::default()),
        ModelSlice {
            slice: &bytes,
            read_start: 0x34,
        },
    )
    .expect("Unable to create ND");
}

#[test]
fn nd_shader_param2() {
    let bytes = get_test_file("test_ndShaderParam2_1");

    let nd = Nd::new(
        &mut ModelReadContext::new(&Default::default()),
        ModelSlice {
            slice: &bytes,
            read_start: 0,
        },
    )
    .expect("Unable to create ND");

    if let NdData::ShaderParam2 {
        main_payload,
        sub_payload,
    } = &*nd.data
    {
        let attribute_map = main_payload.attribute_map();

        assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");

        assert_eq!(
            main_payload.texture_assignments().len()
                + sub_payload
                    .as_ref()
                    .map(|v| v.texture_assignments().len())
                    .unwrap_or(0),
            2,
            "Number of bound textures is wrong."
        );

        assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");
    } else {
        panic!(
            "nd has wrong type {:?}, expected ndShaderParam2.",
            dbg!(&nd)
        );
    }
}
