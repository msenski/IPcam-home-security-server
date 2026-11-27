use std::sync::Arc;

use webrtc::peer_connection::{PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent};


struct EventHandler;


#[async_trait::async_trait]
impl PeerConnectionEventHandler for EventHandler {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        println!("New local ICE candidate gathered: {}", event.candidate);
    }
}

pub async fn try_webrtc() -> anyhow::Result<()> {

    
    // The STUN server will tell us (our server) what IP address and port we appear as to the
    // outside world. This info will be used by the browser to know how to reach us. 
    let config = RTCConfigurationBuilder::default() .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }])
        .build();

    // Create our server's side of the connection. We register the event handler. By binding on 0.0.0.0:0 we
    // tell our OS to choose the current local address (except loopback) and a suitable UDP port ad-hoc, at 
    // connection time. This info will be part of our ICE candidate, which idescribes each way the other
    // peer can reach us. 
    let pc = PeerConnectionBuilder::new()
        .with_configuration(config)
        .with_handler(Arc::new(EventHandler)).with_udp_addrs(vec!["0.0.0.0:0"])
        .build()
        .await?;

    // Create an SDP offer and set it as local description
    let offer = pc.create_offer(None).await?;
    pc.set_local_description(offer).await?;
     
    println!("Local description set successfully!");


    Ok(())


}
