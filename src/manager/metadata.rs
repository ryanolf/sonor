//! Guess metadata and uri from strings
use xml::escape::escape_str_pcdata;
use urlencoding::encode;


fn get_metadata(id: &str, parent_id: &str, upnp_class: &str, cdudn: &str) -> String {
    escape_str_pcdata(&format!(concat!(
        r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-rinconnetworks-com:metadata-1-0/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/">"#,
            r#"<item id="{id}" restricted="true" parentID="{parent_id}">"#,
                r#"<dc:title></dc:title>"#,
                r#"<upnp:class>{upnp_class}</upnp:class>"#,
                r#"<desc id="cdudn" nameSpace="urn:schemas-rinconnetworks-com:metadata-1-0/">{cdudn}</desc>"#,
            r#"</item>"#,
        r#"</DIDL-Lite>"#), 
        id=id, parent_id=parent_id, upnp_class=upnp_class, cdudn=cdudn)).to_string()
  }

  pub(crate) fn spotify_uri_and_metadata(item: &str) -> Option<(String, String)> {
    // return Some(guess_uri_and_metadata(item));
    let (kind, id) = item.split_once(':')?;
    log::debug!("Got {} of {}", id, kind);
    let item = encode(item);
    let cdudn = format!(r"SA_RINCON{region}_X_#Svc{region}-0-Token", region="2311");
    match kind {
        "album" => Some((
            format!(r"x-rincon-cpcontainer:1004206c{}?sid=9&flags=8300&sn=7", item), 
            get_metadata(
                &format!(r"0004206c{}", item),
                r"", 
                r"object.container.album.musicAlbum",
                &cdudn
            )
         )),
         "track" => Some((
            format!(r"x-sonos-http:{}?sid=9&flags=8300&sn=7", item), 
            get_metadata(
                &format!(r"00032020{}", item),
                r"", 
                r"object.item.audioItem.musicTrack",
                &cdudn
            )
         )),
         "playlist" => Some((
            format!(r"x-rincon-cpcontainer:1006206{}??sid=9&flags=8300&sn=7", item), 
            get_metadata(
                &format!(r"10062a6c{}", item),
                r"10fe2664playlists", 
                r"object.container.playlistContainer",
                &cdudn
            )
         )),
         _ => None
    }
}

pub(crate) fn apple_uri_and_metadata(item: &str) -> Option<(String, String)> {
    // return Some(guess_uri_and_metadata(item));
    let (kind, id) = match item.split_once(':')? {
        ("track" , id) => ("song", id),
        (kind, id) => (kind, id)
    };
    log::debug!("Got {} of {}", id, kind);
    let item = format!("{}:{}", kind, id);
    let item = encode(&item);
    let cdudn = format!(r"SA_RINCON{region}_X_#Svc{region}-0-Token", region="52231");
    match kind {
        "album" | "libraryalbum" => Some((
            format!(r"x-rincon-cpcontainer:0004206c{}?sid=204", item), 
            get_metadata(
                &format!(r"0004206c{}", item),
                r"00020000album%3A",
                r"object.item.audioItem.musicAlbum",
                &cdudn
            )
         )),
         "song" | "librarytrack" => Some((
            format!(r"x-sonos-http:{}.mp4?sid=204", item), 
            get_metadata(
                &format!(r"10032020{}", item),
                r"1004206calbum%3A", 
                r"object.item.audioItem.musicTrack",
                &cdudn
            )
         )),
         "playlist" | "libraryplaylist" => Some((
            format!(r"x-rincon-cpcontainer:1006206c{}?sid=204", item), 
            get_metadata(
                &format!(r"1006206c{}", item),
                r"00020000playlist%3A", 
                r"object.container.playlistContainer",
                &cdudn
            )
         )),
         _ => None
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use std::{error::Error};


    #[test]
    fn test_apple_playlist() -> Result<(), Box<dyn Error>> {
        let (uri, meta) = apple_uri_and_metadata("album:1025210938").ok_or("Error")?;
        assert_eq!(uri, r"x-rincon-cpcontainer:1004206calbum:1025210938?sid=204");
        assert_eq!(&meta[250..350], r#"5210938" parentID="00020000album%3a" restricted="true">&lt;dc:title>&lt;/dc:title>&lt;upnp:class>obj"#);
        Ok(())
    }

    #[test]
    fn test_apple_librarytrack() -> Result<(), Box<dyn Error>> {
        let target_uri = "x-sonos-http:librarytrack%3Aa.1442979904.mp4?sid=204";
        let target_metadata = r#"<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-rinconnetworks-com:metadata-1-0/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/"><item id="10032020librarytrack%3Aa.1442979904" restricted="true" parentID="1004206calbum%3A"><dc:title></dc:title><upnp:class>object.item.audioItem.musicTrack</upnp:class><desc id="cdudn" nameSpace="urn:schemas-rinconnetworks-com:metadata-1-0/">SA_RINCON52231_X_#Svc52231-0-Token</desc></item></DIDL-Lite>"#;
        let (uri, metadata) = apple_uri_and_metadata(r"librarytrack:a.1442979904").ok_or("unable to parse item")?;
        assert_eq!(target_uri, uri);
        assert_eq!(escape_str_pcdata(target_metadata), metadata);
        Ok(())
    }
}