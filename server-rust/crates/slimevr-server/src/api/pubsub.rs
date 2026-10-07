//! Bounded topic registry; subscriptions and sender identity belong to each connection.
use solarxr_protocol::{self as sx, datatypes as dt, flatbuffers as fb, pub_sub as ps};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub struct Broker {
    topics: BTreeMap<(String, String, String), u16>,
    handles: BTreeMap<u16, (String, String, String)>,
}
pub struct Dispatch {
    pub replies: Vec<Vec<u8>>,
    pub messages: Vec<(u16, Vec<u8>)>,
}
impl Broker {
    fn topic(&mut self, id: ps::TopicId<'_>) -> Result<u16, String> {
        let key = (
            id.organization()
                .ok_or("missing topic organization")?
                .to_owned(),
            id.app_name().ok_or("missing topic app")?.to_owned(),
            id.topic().ok_or("missing topic name")?.to_owned(),
        );
        if [&key.0, &key.1, &key.2]
            .into_iter()
            .any(|s| s.len() > 256 || s.is_empty())
        {
            return Err("invalid topic identifier".into());
        }
        if let Some(handle) = self.topics.get(&key) {
            return Ok(*handle);
        }
        if self.topics.len() >= 512 {
            return Err("topic limit reached".into());
        }
        let handle = self.topics.len() as u16 + 1;
        self.topics.insert(key.clone(), handle);
        self.handles.insert(handle, key);
        Ok(handle)
    }
    fn handle(&self, id: u16) -> Result<u16, String> {
        self.handles
            .contains_key(&id)
            .then_some(id)
            .ok_or("unknown topic handle".into())
    }
    fn mapping(&self, handle: u16) -> Vec<u8> {
        let key = &self.handles[&handle];
        frame(ps::PubSubUnion::TopicMapping, |f| {
            let organization = f.create_string(&key.0);
            let app_name = f.create_string(&key.1);
            let topic = f.create_string(&key.2);
            let id = ps::TopicId::create(
                f,
                &ps::TopicIdArgs {
                    organization: Some(organization),
                    app_name: Some(app_name),
                    topic: Some(topic),
                },
            );
            let handle = ps::TopicHandle::create(f, &ps::TopicHandleArgs { id: handle });
            ps::TopicMapping::create(
                f,
                &ps::TopicMappingArgs {
                    id: Some(id),
                    handle: Some(handle),
                },
            )
            .as_union_value()
        })
    }
    pub fn dispatch(
        &mut self,
        headers: fb::Vector<'_, fb::ForwardsUOffset<ps::PubSubHeader<'_>>>,
        subscriptions: &mut BTreeSet<u16>,
    ) -> Result<Dispatch, String> {
        if headers.len() > 32 {
            return Err("too many pub/sub headers".into());
        }
        let mut d = Dispatch {
            replies: vec![],
            messages: vec![],
        };
        for h in headers {
            match h.u_type() {
                ps::PubSubUnion::SubscriptionRequest => {
                    let r = h
                        .u_as_subscription_request()
                        .ok_or("missing subscription")?;
                    let handle = if let Some(id) = r.topic_as_topic_id() {
                        self.topic(id)?
                    } else {
                        self.handle(
                            r.topic_as_topic_handle()
                                .ok_or("missing subscription topic")?
                                .id(),
                        )?
                    };
                    if subscriptions.len() >= 128 && !subscriptions.contains(&handle) {
                        return Err("subscription limit reached".into());
                    }
                    subscriptions.insert(handle);
                    d.replies.push(self.mapping(handle));
                }
                ps::PubSubUnion::TopicHandleRequest => {
                    let id = h
                        .u_as_topic_handle_request()
                        .and_then(|r| r.id())
                        .ok_or("missing topic ID")?;
                    let handle = self.topic(id)?;
                    d.replies.push(self.mapping(handle));
                }
                ps::PubSubUnion::Message => {
                    let m = h.u_as_message().ok_or("missing pub/sub message")?;
                    let handle = if let Some(id) = m.topic_as_topic_id() {
                        self.topic(id)?
                    } else {
                        self.handle(
                            m.topic_as_topic_handle()
                                .ok_or("missing message topic")?
                                .id(),
                        )?
                    };
                    let payload = Payload::read(m)?;
                    let bytes = frame(ps::PubSubUnion::Message, |f| {
                        let topic = ps::TopicHandle::create(f, &ps::TopicHandleArgs { id: handle })
                            .as_union_value();
                        let (kind, value) = payload.pack(f);
                        ps::Message::create(
                            f,
                            &ps::MessageArgs {
                                topic_type: ps::Topic::TopicHandle,
                                topic: Some(topic),
                                payload_type: kind,
                                payload: value,
                            },
                        )
                        .as_union_value()
                    });
                    d.messages.push((handle, bytes));
                }
                _ => return Err("unsupported pub/sub header".into()),
            }
        }
        Ok(d)
    }
}
fn frame(
    kind: ps::PubSubUnion,
    build: impl FnOnce(&mut fb::FlatBufferBuilder<'_>) -> fb::WIPOffset<fb::UnionWIPOffset>,
) -> Vec<u8> {
    let mut f = fb::FlatBufferBuilder::new();
    let u = build(&mut f);
    let header = ps::PubSubHeader::create(
        &mut f,
        &ps::PubSubHeaderArgs {
            u_type: kind,
            u: Some(u),
        },
    );
    let headers = f.create_vector(&[header]);
    let bundle = sx::MessageBundle::create(
        &mut f,
        &sx::MessageBundleArgs {
            pub_sub_msgs: Some(headers),
            ..Default::default()
        },
    );
    f.finish(bundle, None);
    f.finished_data().to_vec()
}
enum Payload {
    None,
    Text(String),
    Bytes(Vec<u8>),
    Pairs(Vec<String>, Vec<String>),
}
impl Payload {
    fn read(m: ps::Message<'_>) -> Result<Self, String> {
        match m.payload_type() {
            ps::Payload::NONE => Ok(Self::None),
            ps::Payload::solarxr_protocol_datatypes_StringTable => {
                let s = m
                    .payload_as_solarxr_protocol_datatypes_string_table()
                    .and_then(|v| v.s())
                    .ok_or("missing string payload")?;
                if s.len() > 32768 {
                    return Err("pub/sub string limit exceeded".into());
                }
                Ok(Self::Text(s.into()))
            }
            ps::Payload::solarxr_protocol_datatypes_Bytes => {
                let b = m
                    .payload_as_solarxr_protocol_datatypes_bytes()
                    .and_then(|v| v.b())
                    .ok_or("missing byte payload")?;
                if b.len() > 32768 {
                    return Err("pub/sub byte limit exceeded".into());
                }
                Ok(Self::Bytes(b.bytes().into()))
            }
            ps::Payload::KeyValues => {
                let p = m
                    .payload_as_key_values()
                    .ok_or("missing key/value payload")?;
                let keys = p.keys().ok_or("missing payload keys")?;
                let values = p.values().ok_or("missing payload values")?;
                if keys.len() != values.len()
                    || keys.len() > 128
                    || keys.iter().chain(values.iter()).any(|s| s.len() > 2048)
                {
                    return Err("invalid key/value payload".into());
                }
                Ok(Self::Pairs(
                    keys.iter().map(str::to_owned).collect(),
                    values.iter().map(str::to_owned).collect(),
                ))
            }
            _ => Err("unknown pub/sub payload".into()),
        }
    }
    fn pack<'a>(
        &self,
        f: &mut fb::FlatBufferBuilder<'a>,
    ) -> (ps::Payload, Option<fb::WIPOffset<fb::UnionWIPOffset>>) {
        match self {
            Self::None => (ps::Payload::NONE, None),
            Self::Text(s) => {
                let s = f.create_string(s);
                (
                    ps::Payload::solarxr_protocol_datatypes_StringTable,
                    Some(
                        dt::StringTable::create(f, &dt::StringTableArgs { s: Some(s) })
                            .as_union_value(),
                    ),
                )
            }
            Self::Bytes(b) => {
                let b = f.create_vector(b);
                (
                    ps::Payload::solarxr_protocol_datatypes_Bytes,
                    Some(dt::Bytes::create(f, &dt::BytesArgs { b: Some(b) }).as_union_value()),
                )
            }
            Self::Pairs(keys, values) => {
                let keys = keys.iter().map(|s| f.create_string(s)).collect::<Vec<_>>();
                let values = values
                    .iter()
                    .map(|s| f.create_string(s))
                    .collect::<Vec<_>>();
                let keys = f.create_vector(&keys);
                let values = f.create_vector(&values);
                (
                    ps::Payload::KeyValues,
                    Some(
                        ps::KeyValues::create(
                            f,
                            &ps::KeyValuesArgs {
                                keys: Some(keys),
                                values: Some(values),
                            },
                        )
                        .as_union_value(),
                    ),
                )
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct Hub {
    pub broker: std::sync::Arc<std::sync::Mutex<Broker>>,
    next: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl Hub {
    pub fn client_id(&self) -> u64 {
        self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}
