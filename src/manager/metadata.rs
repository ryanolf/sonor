//! Guess metadata and uri from strings

use xml::escape::escape_str_pcdata;

pub(super) fn guess_uri_and_metadata(_uri: &str) -> (String, String) {
    let uri = "x-sonos-http:librarytrack:a.1442979904.mp4?sid=204";
    let metadata = r#"
<DIDL-Lite xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:r="urn:schemas-rinconnetworks-com:metadata-1-0/" xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/">
    <item id="10032020librarytrack%3aa.1442979904" restricted="true" parentID="1004206calbum%3a">
        <dc:title></dc:title>
        <upnp:class>object.item.audioItem.musicTrack</upnp:class>
        <desc id="cdudn" nameSpace="urn:schemas-rinconnetworks-com:metadata-1-0/">SA_RINCON52231_X_#Svc52231-0-Token</desc>
    </item>
</DIDL-Lite>"#;

    (uri.to_string(), escape_str_pcdata(metadata).to_string())
}
