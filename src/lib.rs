use libmpv::{events::Event, Mpv};
use libmpv_sys::mpv_handle;
use std::{
    fs::File,
    io::Write,
    mem::{transmute, ManuallyDrop},
    process::Command,
    ptr::NonNull,
    sync::atomic::AtomicBool,
};
use tempfile::{tempdir, TempDir};
use url::Url;

#[allow(dead_code)]
struct InnerMpv {
    ctx: NonNull<libmpv_sys::mpv_handle>,
    events_guard: AtomicBool,
}

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let pub_mpv = InnerMpv {
        ctx: NonNull::new(handle).unwrap(),
        events_guard: AtomicBool::new(false),
    };
    let mpv = ManuallyDrop::new(unsafe { transmute::<InnerMpv, Mpv>(pub_mpv) });
    let mut event_context = mpv.create_event_context();

    loop {
        match event_context.wait_event(-1.).unwrap().unwrap() {
            Event::FileLoaded => {
                match Url::parse(&mpv.get_property::<String>("path").unwrap()) {
                    Err(_) => continue,
                    Ok(url) => {
                        if url.domain() != Some("www.bilibili.com") {
                            continue;
                        }
                    }
                }

                remove_xml_sub(&mpv);

                let (temp_dir, mut temp_file) =
                    match create_temp_file(mpv.get_property::<String>("filename").unwrap()) {
                        None => continue,
                        Some(temp_tuple) => temp_tuple,
                    };

                let subtitle = get_danmaku_ass(&mpv.get_property("path").unwrap()).unwrap();
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
    let xml_sub_id_option = (0..mpv
        .get_property::<String>("track-list/count")
        .unwrap()
        .parse()
        .unwrap())
        .find(|track_id| {
            mpv.get_property::<String>(&format!("track-list/{}/type", track_id))
                .unwrap()
                == "sub"
                && mpv.get_property::<String>(&format!("track-list/{}/lang", track_id))
                    == Ok("danmaku".to_owned())
                && mpv.get_property::<String>(&format!("track-list/{}/title", track_id))
                    == Ok("xml".to_owned())
        });
    if let Some(xml_sub_id) = xml_sub_id_option {
        let sub_id = mpv
            .get_property::<String>(&format!("track-list/{}/id", xml_sub_id))
            .unwrap();
        mpv.command("sub-remove", &[&sub_id]).unwrap();
    }
}

fn create_temp_file(filename: String) -> Option<(TempDir, File)> {
    let temp_directory = tempdir().ok()?;
    let temp_path_buf = temp_directory.path().join(filename).with_extension("ass");
    Some((temp_directory, File::create(temp_path_buf).ok()?))
}

fn get_danmaku_ass(path: &String) -> Option<Vec<u8>> {
    let output = Command::new("danmu2ass")
        .args(["-o", "-", path])
        .output();
    match output {
        Ok(output) => {
            println!("{}", String::from_utf8(output.stderr).unwrap());
            Some(output.stdout)
        }
        Err(err) => {
            println!("{}", err);
            None
        }
    }
}
