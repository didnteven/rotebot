#![cfg(unix)]
use macroscope_muse_client::*;
use macroscope_vault::Secret;
use std::os::unix::fs::PermissionsExt;
#[tokio::test]
async fn fake_msp_handshake_correlates_turn_and_reaps_without_credential_disclosure() {
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let script = root.path().join("fake-msp");
    let source=r#"#!/usr/bin/python3
import sys,json,os
assert sys.argv[1:] == ['serve','--provider','echo','--contributor-mode','off','--sandbox-network','restricted','--disable-shell']
def send(v): print(json.dumps(v),flush=True)
def event(method,params): send({'jsonrpc':'2.0','method':method,'params':dict(sessionId='s',**params)})
for line in sys.stdin:
    v=json.loads(line); m=v['method']; p=v.get('params',{})
    assert m in ['initialize','initialized','model/list','session/start','turn/start','turn/interrupt']
    if m=='initialized': continue
    if m=='initialize':
        assert p['capabilities']['requestedCapabilities']==[]
        r={'schema':{'version':1,'fingerprint':'PINNED'},'sandboxAvailability':'available'}
    elif m=='model/list': r={'models':[]}
    elif m=='session/start': r={'session':{'sessionId':'s'}}
    elif m=='turn/start': r={'status':'accepted','turnId':'t'}
    else: r={'status':'accepted','turnId':'t'}
    send({'jsonrpc':'2.0','id':v['id'],'result':r})
    if m=='turn/start':
        event('turn/completed',{'turnId':'stale','terminal':'completed'})
        event('item/started',{'item':{'itemId':'i','turnId':'t','kind':'agentMessage','revision':1,'status':'inProgress','text':''}})
        event('item/delta',{'itemId':'i','delta':'partial'})
        event('item/completed',{'item':{'itemId':'i','turnId':'t','kind':'agentMessage','revision':2,'status':'completed','text':'echo: hello from macroscope','reflectedCredential':os.environ['META_API_KEY']}})
        event('turn/foregroundCompleted',{'turnId':'t'})
        event('session/tokenUsage',{'turnId':'t','usage':{'inputTokens':0,'outputTokens':0}})
        event('turn/completed',{'turnId':'t','terminal':'completed'})
"#.replace("PINNED",PINNED_FINGERPRINT);
    std::fs::write(&script, source).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let binary = BinaryInfo {
        path: script,
        version: "1.4.11".into(),
    };
    let env = ChildEnv::for_route(
        CredentialRoute::ApiKey,
        &root.path().join("home"),
        root.path(),
        Some(Secret::new(b"bogus-msp-canary".to_vec())),
    )
    .unwrap();
    let mut client = MspClient::spawn(&binary, Provider::Echo, env, root.path())
        .await
        .unwrap();
    assert!(client.model_list().await.unwrap()["models"].is_array());
    let session = client.session_start(root.path()).await.unwrap();
    client
        .turn_start(&session, "hello from macroscope")
        .await
        .unwrap();
    assert_eq!(
        client.wait_turn().await.unwrap(),
        "echo: hello from macroscope"
    );
    assert!(client.reducer.usage.is_some());
    assert!(!format!("{:?}", client.reducer).contains("bogus-msp-canary"));
    client.shutdown().await.unwrap();
}
