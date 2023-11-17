use libmpv::{events::Event, Mpv};
use libmpv_sys::mpv_handle;
use std::{fs::File, io::Write, process::Command, str};
use tempfile::{tempdir, TempDir};
use url::Url;

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let mut mpv = Mpv::new_with_context(handle).unwrap();

    loop {
        let event_context = mpv.event_context_mut();

        let Some(Ok(event)) = event_context.wait_event(-1.) else {
            continue;
        };

        match event {
            Event::FileLoaded => {
                let media_path_origin = mpv.get_property::<String>("path").unwrap();
                match Url::parse(&media_path_origin) {
                    Err(_) => continue,
                    Ok(url) => {
                        if url.domain() != Some("www.bilibili.com") {
                            continue;
                        }
                    }
                }
                let media_path = media_path_origin.trim_end_matches('/');

                remove_xml_sub(&mpv);

                let (temp_dir, mut temp_file) =
                    match create_temp_file(&mpv.get_property::<String>("filename").unwrap()) {
                        None => continue,
                        Some(temp_tuple) => temp_tuple,
                    };

                let subtitle = get_danmaku_ass(media_path).unwrap();
                temp_file.write_all(&subtitle).unwrap();

                mpv.set_property("sub-auto", "exact").unwrap();
                mpv.set_property("options/sub-file-paths", temp_dir.path().to_str().unwrap())
                    .unwrap();
                mpv.command("rescan-external-files", &["reselect"]).ok();
            }
            Event::Shutdown => {
                return 0;
            }
            _ => {}
        }
    }
}

fn remove_xml_sub(mpv: &Mpv) {
    let xml_sub_id_option =
        (0..mpv.get_property::<i64>("track-list/count").unwrap()).find(|track_id| {
            mpv.get_property::<String>(&format!("track-list/{track_id}/type"))
                == Ok("sub".to_owned())
                && mpv.get_property::<String>(&format!("track-list/{track_id}/lang"))
                    == Ok("danmaku".to_owned())
                && mpv.get_property::<String>(&format!("track-list/{track_id}/title"))
                    == Ok("xml".to_owned())
        });

    if let Some(xml_sub_id) = xml_sub_id_option {
        let sub_id = mpv
            .get_property::<String>(&format!("track-list/{xml_sub_id}/id"))
            .unwrap();
        mpv.command("sub-remove", &[&sub_id]).unwrap();
    }
}

fn create_temp_file(filename: &str) -> Option<(TempDir, File)> {
    let temp_directory = tempdir().ok()?;
    let temp_path_buf = temp_directory.path().join(filename).with_extension("ass");
    Some((temp_directory, File::create(temp_path_buf).ok()?))
}

fn get_danmaku_ass(path: &str) -> Option<Vec<u8>> {
    let output = Command::new("danmu2ass").args(["-o", "-", path]).output();

    match output {
        Ok(output) => {
            println!("{}", str::from_utf8(&output.stderr).unwrap());
            Some(output.stdout)
        }
        Err(err) => {
            println!("{err}");
            None
        }
    }
}
