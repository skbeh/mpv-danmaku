use libmpv::{events::Event, Mpv};
use libmpv_sys::mpv_handle;
use std::{
    env::temp_dir,
    fs::{remove_file, File},
    io::Write,
    process::Command,
    ptr::NonNull,
    sync::atomic::AtomicBool,
};
use url::Url;

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let mpv = Mpv {
        ctx: NonNull::new(handle).unwrap(),
        events_guard: AtomicBool::new(false),
    };
    let mut event_context = mpv.create_event_context();
    let mut path_vec: Vec<String> = Vec::new();

    loop {
        match event_context.wait_event(-1.).unwrap().unwrap() {
            Event::FileLoaded => {
                println!("{}", mpv.get_property::<String>("path").unwrap());
                match Url::parse(&mpv.get_property::<String>("path").unwrap()) {
                    Err(_) => continue,
                    Ok(url) => {
                        if url.domain() != Some("www.bilibili.com") {
                            continue;
                        }
                    }
                }

                remove_xml_sub(&mpv);

                let temp_directory = temp_dir();
                let temp_path_buf = temp_directory
                    .join(mpv.get_property::<String>("filename").unwrap())
                    .with_extension("ass");
                path_vec.push(temp_path_buf.to_str().unwrap().to_owned());
                let mut temp_file = File::create(temp_path_buf).unwrap();

                let subtitle = get_danmaku_ass(&mpv.get_property("path").expect("test0")).unwrap();
                temp_file.write_all(&subtitle).unwrap();

                mpv.set_property("sub-auto", "exact").unwrap();
                mpv.set_property("options/sub-file-paths", temp_directory.to_str().unwrap())
                    .unwrap();
                mpv.command("rescan-external-files", &["reselect"]).ok();
            }
            Event::Shutdown => {
                path_vec.iter().for_each(|path| {
                    remove_file(path).ok();
                });
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

fn get_danmaku_ass(path: &String) -> Option<Vec<u8>> {
    let output = Command::new("danmu2ass").args(["-o", "-", path]).output();
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
