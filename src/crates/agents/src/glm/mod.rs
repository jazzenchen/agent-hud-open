//! GLM. GLM is a configured billing provider, not an installed execution client.
pub mod discovery;

use crate::Agent;
use crate::Discovery;

static DISCOVERY: Discovery = Discovery {
    credentials_env: &[
        "ZAI_API_KEY",
        "ZAI_CODING_CN_API_KEY",
        "BIGMODEL_API_KEY",
        "ZHIPU_API_KEY",
        "ZHIPUAI_API_KEY",
        "GLM_API_KEY",
    ],
    configured: Some(discovery::configured),
    ..Discovery::NONE
};

pub static AGENT: Agent = Agent {
    id: "glm",
    name: "GLM",
    sessions: false,
    hooks: None,
    observer: None,
    channel: None,
    program: None,
    replies: None,
    turns: None,
    items: None,
    sources: crate::everywhere(&[]),
    discovery: &DISCOVERY,
};
