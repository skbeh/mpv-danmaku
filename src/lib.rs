use libmpv::{events::Event, Mpv};
use libmpv_sys::mpv_handle;
use std::{fs::File, io::Write, mem::ManuallyDrop, path::PathBuf, process::Command, str};
use tempfile::{tempdir, TempDir};
use url::Url;

#[no_mangle]
extern "C" fn mpv_open_cplugin(handle: *mut mpv_handle) -> std::os::raw::c_int {
    let mut mpv = ManuallyDrop::new(Mpv::new_with_context(handle).unwrap());

    loop {
        let Some(Ok(event)) = mpv.event_context_mut().wait_event(-1.) else {
            continue;
        };

        match event {
            Event::FileLoaded => {
                if let Err(err) = load_sub(&mpv) {
                    eprintln!("Failed to load danmaku subtitle: {err}");
                }
            }
            Event::Shutdown => {
                return 0;
            }
            _ => {}
        }
    }
}

fn is_av_or_bv(video_id: &str) -> bool {
    video_id.len() > 2
        && (video_id.starts_with("av") || video_id.to_ascii_lowercase().starts_with("bv"))
}

fn load_sub(mpv: &Mpv) -> Result<(), Box<dyn std::error::Error>> {
    let mut media_url = match Url::parse(
        mpv.get_property::<String>("path")
            .unwrap()
            .trim_end_matches('/'),
    ) {
        Err(_) => return Ok(()),
        Ok(url) => url,
    };

    if media_url.cannot_be_a_base() {
        return Ok(());
    }

    if media_url.domain() == Some("bilibili.com") {
        media_url.set_host(Some("www.bilibili.com")).unwrap();
    } else if media_url.domain() != Some("www.bilibili.com") {
        return Ok(());
    }

    if media_url.path().starts_with("/video/") {
        if let Some(id) = media_url.path_segments().unwrap().nth(1) {
            if !is_av_or_bv(id) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    } else if let Some(id) = media_url.path_segments().unwrap().next() {
        if is_av_or_bv(id) {
            media_url.set_path(("/video".to_owned() + media_url.path()).as_str());
        } else if id == "festival" {
            if let Some(bvid) = media_url.query_pairs().find_map(|pair| {
                if pair.0 == "bvid" {
                    Some(pair.1.into_owned())
                } else {
                    None
                }
            }) {
                media_url = Url::parse("https://www.bilibili.com/video").unwrap();
                media_url.path_segments_mut().unwrap().push(bvid.as_str());
            }
        } else if id != "bangumi" {
            return Ok(());
        }
    } else {
        return Ok(());
    }

    remove_xml_sub(mpv);

    let last_url_segment = match media_url.path_segments() {
        None => return Ok(()),
        Some(mut segments) => match segments.next_back() {
            None => return Ok(()),
            Some(segment) => segment,
        },
    };

    let (_temp_dir, mut temp_file, temp_file_path) = create_temp_file(last_url_segment)?;

    let mut media_url_with_bv = media_url.clone();
    if let Some(avid_str) = last_url_segment.strip_prefix("av") {
        if let Ok(avid) = avid_str.parse::<u64>() {
            media_url_with_bv
                .path_segments_mut()
                .unwrap()
                .pop()
                .push(abv::av2bv(avid)?.as_str());
        } else {
            return Ok(());
        }
    }

    let subtitle =
        get_danmaku_ass(media_url_with_bv.as_str()).ok_or("Failed to get danmaku ass")?;
    temp_file.write_all(&subtitle)?;
    temp_file.flush()?;

    mpv.set_property("sub-auto", "exact").unwrap();
    mpv.subtitle_add_select(
        temp_file_path.to_str().unwrap(),
        Some("danmaku"),
        Some("chs"),
    )
    .unwrap();

    Ok(())
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
            .unwrap()
            .parse::<usize>()
            .unwrap();
        mpv.subtitle_remove(Some(sub_id)).ok();
    }
}

fn create_temp_file(filename: &str) -> std::io::Result<(TempDir, File, PathBuf)> {
    let temp_directory = tempdir()?;
    let temp_path_buf = temp_directory.path().join(filename).with_extension("ass");
    Ok((temp_directory, File::create(&temp_path_buf)?, temp_path_buf))
}

fn get_danmaku_ass(path: &str) -> Option<Vec<u8>> {
    let output = Command::new("danmu2ass")
        .args(["--no-web", "-o", "-", path])
        .output();

    match output {
        Ok(output) => {
            println!("{}", String::from_utf8_lossy(&output.stderr));
            if !output.status.success() {
                println!("{}", String::from_utf8_lossy(&output.stdout));
                return None;
            }
            Some(output.stdout)
        }
        Err(err) => {
            println!("{err}");
            None
        }
    }
}
