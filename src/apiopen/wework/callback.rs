//! WeWork Message Callback
//!
//! Enterprise WeChat message receiving callback
//!
//! Route: /apiopen/wework/callback/:apifun
//! - GET  /apiopen/wework/callback/index - URL verification
//! - POST /apiopen/wework/callback/index - Receive messages

use axum::{
    body::Bytes,
    http::{Method, StatusCode, header},
};
use base::Response;
use sha1::{Sha1, Digest};
use std::collections::HashMap;
use crate::get_wework_config;
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use base64::{Engine as _, engine::general_purpose};

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

/// Custom base64 decode that ignores invalid trailing bits (for WeWork 43-char key)
/// WeWork's encoding_aes_key has invalid bits in the last character that Rust base64 0.22 rejects
fn decode_base64_lenient(input: &str) -> Result<Vec<u8>, String> {
    // Standard base64 alphabet
    const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    
    let input = input.trim();
    let input = input.trim_end_matches('=');
    
    // Build decode table
    let mut decode_table = [0xFFu8; 256];
    for (i, &c) in BASE64_ALPHABET.iter().enumerate() {
        decode_table[c as usize] = i as u8;
    }
    
    // Calculate output length
    let output_len = (input.len() * 3) / 4;
    let mut output = Vec::with_capacity(output_len);
    
    let chars: Vec<u8> = input.bytes().collect();
    let chunks = chars.chunks(4);
    
    for chunk in chunks {
        let mut acc: u32 = 0;
        let mut bits = 0;
        
        for &c in chunk {
            let val = decode_table[c as usize];
            if val == 0xFF {
                return Err(format!("Invalid base64 character: {}", c as char));
            }
            acc = (acc << 6) | (val as u32);
            bits += 6;
        }
        
        // Output complete bytes
        while bits >= 8 {
            bits -= 8;
            output.push((acc >> bits) as u8);
        }
    }
    
    Ok(output)
}

/// Handle raw HTTP request (no middleware)
pub async fn handle_raw(
    apifun: &str,
    method: &Method,
    query: &HashMap<String, String>,
    body: Bytes,
) -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 1], Bytes) {
    match apifun.to_lowercase().as_str() {
        "index" | "verify" | "send" => {
            if method == Method::GET {
                // URL verification
                verify_url(query).await
            } else {
                // POST - receive message
                receive_message(query, body).await
            }
        }
        _ => {
            let resp = Response::fail(&format!("API not found: {}", apifun), 404);
            (StatusCode::NOT_FOUND, [(header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default()))
        }
    }
}

/// Handle GET request - URL verification
async fn verify_url(params: &std::collections::HashMap<String, String>) -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 1], Bytes) {
    // 打印收到的所有参数
    tracing::info!("=== WeWork verify_url called ===");
    tracing::info!("All params: {:?}", params);
    
    let config = get_wework_config();
    tracing::info!("Config: token={}, encoding_aes_key={}, corp_id={}", config.token, config.encoding_aes_key, config.corp_id);
    
    // URL decode all parameters (企业微信要求)
    let msg_signature = url_decode(params.get("msg_signature").map(|s| s.as_str()).unwrap_or(""));
    let timestamp = url_decode(params.get("timestamp").map(|s| s.as_str()).unwrap_or(""));
    let nonce = url_decode(params.get("nonce").map(|s| s.as_str()).unwrap_or(""));
    let echostr = url_decode(params.get("echostr").map(|s| s.as_str()).unwrap_or(""));
    
    tracing::info!("Decoded params: msg_signature={}, timestamp={}, nonce={}, echostr={}", msg_signature, timestamp, nonce, echostr);
    tracing::info!("echostr length: {}, first 50 chars: {}", echostr.len(), &echostr.chars().take(50).collect::<String>());
    
    // Verify signature (URL验证需要包含echostr)
    if !verify_signature(&config.token, &timestamp, &nonce, &msg_signature, Some(&echostr)) {
        tracing::error!("WeWork verify failed: invalid signature");
        return (StatusCode::FORBIDDEN, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from("Invalid signature".to_string()));
    }
    
    tracing::info!("Signature verified OK");
    
    // Decrypt echostr - WeWork always encrypts echostr during URL verification
    if !config.encoding_aes_key.is_empty() {
        match decrypt_echostr(&config.encoding_aes_key, &config.corp_id, &echostr) {
            Ok(decrypted) => {
                tracing::info!("WeWork verify success: returning decrypted message: {}", decrypted);
                (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from(decrypted))
            }
            Err(e) => {
                tracing::error!("WeWork decrypt failed: {}", e);
                // If decrypt fails, try returning echostr directly (for safe=0 mode)
                (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from(echostr.to_string()))
            }
        }
    } else {
        tracing::info!("WeWork verify success: returning echostr (no encoding_aes_key)");
        (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from(echostr.to_string()))
    }
}

/// Handle POST request - Receive message
async fn receive_message(params: &std::collections::HashMap<String, String>, body: Bytes) -> (StatusCode, [(axum::http::header::HeaderName, &'static str); 1], Bytes) {
    let config = get_wework_config();
    
    let msg_signature = params.get("msg_signature").map(|s| s.as_str()).unwrap_or("");
    let timestamp = params.get("timestamp").map(|s| s.as_str()).unwrap_or("");
    let nonce = params.get("nonce").map(|s| s.as_str()).unwrap_or("");
    
    tracing::info!("WeWork message params: msg_signature={}, timestamp={}, nonce={}", msg_signature, timestamp, nonce);
    
    // Parse message body
    let body_str = match String::from_utf8(body.to_vec()) {
        Ok(s) => s,
        Err(_) => {
            let resp = Response::fail("Invalid body", -1);
            return (StatusCode::BAD_REQUEST, [(axum::http::header::CONTENT_TYPE, "application/json")], Bytes::from(serde_json::to_string(&resp).unwrap_or_default()));
        }
    };
    
    tracing::info!("WeWork message received: {}", body_str);
    
    // Extract Encrypt tag for signature verification
    let encrypt_content = extract_encrypt_tag(&body_str).unwrap_or("");
    
    // Verify signature (消息接收需要包含 encrypt 内容)
    if !verify_signature(&config.token, timestamp, nonce, msg_signature, Some(encrypt_content)) {
        tracing::error!("WeWork message verify failed: invalid signature");
        return (StatusCode::FORBIDDEN, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from("Invalid signature".to_string()));
    }
    
    // Decrypt if encoding_aes_key is set
    let message_xml = if !config.encoding_aes_key.is_empty() {
        match decrypt_xml_message(&config.encoding_aes_key, &config.corp_id, &body_str) {
            Ok(xml) => xml,
            Err(e) => {
                tracing::error!("WeWork decrypt failed: {}", e);
                // Fall back to raw body if decrypt fails
                body_str
            }
        }
    } else {
        body_str
    };
    
    // Parse message
    let msg = match parse_message(&message_xml) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("WeWork parse failed: {}", e);
            return (StatusCode::BAD_REQUEST, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from(format!("Parse failed: {}", e)));
        }
    };
    
    tracing::info!("WeWork message parsed: {:?}", msg);
    
    // Handle message
    let reply = handle_message(&msg).await;
    
    // Return reply or success
    if let Some(reply_content) = reply {
        // Encrypt the reply
        if !config.encoding_aes_key.is_empty() {
            match build_encrypted_reply(&config.token, &config.encoding_aes_key, &config.corp_id, &msg.from_user, &msg.to_user, &reply_content) {
                Ok(encrypted_xml) => {
                    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/xml")], Bytes::from(encrypted_xml))
                },
                Err(e) => {
                    tracing::error!("Failed to encrypt reply: {}", e);
                    (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from("success".to_string()))
                }
            }
        } else {
            (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "application/xml")], Bytes::from(reply_content))
        }
    } else {
        (StatusCode::OK, [(axum::http::header::CONTENT_TYPE, "text/plain")], Bytes::from("success".to_string()))
    }
}

/// Parse query string into HashMap
fn parse_query_params(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let value = parts.next().map(|v| urlencoding_decode(v)).unwrap_or_default();
            Some((key, value))
        })
        .collect()
}

/// 菜单配置项
#[derive(Debug, Clone)]
pub struct MenuSubjectConfig {
    pub grade: String,
    pub subject: String,
}

/// 获取每日一炼菜单配置
fn get_daily_menu_config() -> std::collections::HashMap<String, MenuSubjectConfig> {
    use std::fs;
    
    let mut config = std::collections::HashMap::new();
    
    // 尝试读取配置文件
    let config_path = "docs/config/development.ini";
    if let Ok(content) = fs::read_to_string(config_path) {
        let mut in_section = false;
        for line in content.lines() {
            let line = line.trim();
            
            // 检测 section
            if line == "[DAILY_MENU]" {
                in_section = true;
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                in_section = false;
                continue;
            }
            
            // 解析 key = value
            if in_section && line.contains('=') {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let value = parts[1].trim();
                    // 格式: 年级:科目
                    let value_parts: Vec<&str> = value.split(':').collect();
                    if value_parts.len() == 2 {
                        config.insert(key.to_string(), MenuSubjectConfig {
                            grade: value_parts[0].trim().to_string(),
                            subject: value_parts[1].trim().to_string(),
                        });
                    }
                }
            }
        }
    }
    
    tracing::debug!("加载菜单配置: {} 项", config.len());
    config
}

/// Simple URL decode
fn urlencoding_decode(s: &str) -> String {
    s.replace("%20", " ")
        .replace("%2B", "+")
        .replace("%2F", "/")
        .replace("%3D", "=")
        .replace("%3F", "?")
        .replace("%26", "&")
        .replace("%25", "%")
}

/// URL decode helper (alias)
fn url_decode(s: &str) -> String {
    urlencoding_decode(s)
}

/// WeWork message types
#[derive(Debug, Clone)]
pub struct WeWorkMessage {
    pub to_user: String,
    pub from_user: String,
    pub create_time: i64,
    pub msg_type: String,
    pub content: Option<String>,
    pub msg_id: Option<i64>,
    pub agent_id: Option<i64>,
    pub event: Option<String>,
    pub event_key: Option<String>,
    pub change_type: Option<String>,
}

/// Parse XML message
fn parse_message(xml: &str) -> Result<WeWorkMessage, String> {
    // Simple XML parsing - handles both CDATA and plain text
    let get_tag = |tag: &str| -> Option<String> {
        // Try CDATA first: <tag><![CDATA[value]]></tag>
        let cdata_start = format!("<{}><![CDATA[", tag);
        if let Some(s) = xml.find(&cdata_start) {
            let start_idx = s + cdata_start.len();
            if let Some(e) = xml[start_idx..].find("]]>") {
                return Some(xml[start_idx..start_idx+e].to_string());
            }
        }
        
        // Try plain text: <tag>value</tag>
        let start = format!("<{}>", tag);
        let end = format!("</{}>", tag);
        if let Some(s) = xml.find(&start) {
            if let Some(e) = xml.find(&end) {
                let start_idx = s + start.len();
                if start_idx < e {
                    let value = xml[start_idx..e].to_string();
                    // Strip CDATA wrapper if present (for malformed XML)
                    if value.starts_with("<![CDATA[") && value.ends_with("]]>") {
                        return Some(value[9..value.len()-3].to_string());
                    }
                    return Some(value);
                }
            }
        }
        None
    };
    
    Ok(WeWorkMessage {
        to_user: get_tag("ToUserName").unwrap_or_default(),
        from_user: get_tag("FromUserName").unwrap_or_default(),
        create_time: get_tag("CreateTime").and_then(|s| s.parse().ok()).unwrap_or(0),
        msg_type: get_tag("MsgType").unwrap_or_default(),
        content: get_tag("Content"),
        msg_id: get_tag("MsgId").and_then(|s| s.parse().ok()),
        agent_id: get_tag("AgentID").and_then(|s| s.parse().ok()),
        event: get_tag("Event"),
        event_key: get_tag("EventKey"),
        change_type: get_tag("ChangeType"),
    })
}

/// Handle received message
async fn handle_message(msg: &WeWorkMessage) -> Option<String> {
    match msg.msg_type.as_str() {
        "text" => {
            // Text message - 检查是否有答题状态
            let content = msg.content.as_deref().unwrap_or("");
            tracing::info!("Text message from {}: {}", msg.from_user, content);
            
            // 检查是否有答题状态
            match handle_daily_quiz_judge(&msg.from_user, content).await {
                Some(reply) => Some(reply),
                None => Some(format!("收到: {}", content)),
            }
        }
        "event" => {
            // Event message - 先登录用户
            let user_info = login_user(&msg.from_user).await;
            
            match msg.event.as_deref() {
                Some("subscribe") => {
                    tracing::info!("User {} subscribed", msg.from_user);
                    match user_info {
                        Ok(user) => Some(format!("欢迎关注！您的SID: {}", user.sid)),
                        Err(e) => {
                            tracing::error!("Login failed: {}", e);
                            Some("欢迎关注！登录失败，请重试".to_string())
                        }
                    }
                }
                Some("unsubscribe") => {
                    tracing::info!("User {} unsubscribed", msg.from_user);
                    None
                }
                Some("click") => {
                    tracing::info!("User {} clicked menu: {:?}", msg.from_user, msg.event_key);
                    // 根据 EventKey 路由到不同业务
                    handle_menu_click(&msg.from_user, msg.event_key.as_deref(), user_info).await
                }
                Some("enter_chat") => {
                    tracing::info!("User {} entered chat", msg.from_user);
                    match user_info {
                        Ok(user) => Some(format!("进入会话！SID: {}", user.sid)),
                        Err(_) => Some("进入会话失败".to_string()),
                    }
                }
                Some("change_external_contact") => {
                    tracing::info!("External contact change: {:?}", msg.change_type);
                    None
                }
                _ => {
                    tracing::info!("Unknown event: {:?}", msg.event);
                    None
                }
            }
        }
        _ => {
            tracing::info!("Unknown message type: {}", msg.msg_type);
            None
        }
    }
}

/// 登录用户（查找或创建）
async fn login_user(wechat_userid: &str) -> Result<crate::UserInfo, String> {
    use crate::LoversDataStateMysql;
    
    let state = LoversDataStateMysql::new()?;
    let config = get_wework_config();
    
    // 内部应用使用 internal 类型
    state.find_or_create_user(wechat_userid, "internal", &config.corp_id)
}

/// 处理菜单点击事件
async fn handle_menu_click(
    wechat_userid: &str,
    event_key: Option<&str>,
    user_info: Result<crate::UserInfo, String>,
) -> Option<String> {
    let user = match user_info {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("User not logged in: {}", e);
            return Some("请先关注公众号".to_string());
        }
    };
    
    let event_key = match event_key {
        Some(k) => k,
        None => return None,
    };
    
    // 解析 EventKey: 格式可能是 "#sendmsg#_0_0#7599827016206112" 或直接 "7599827016206112"
    let menu_id = if event_key.starts_with('#') {
        // 格式: #sendmsg#_0_0#菜单ID
        event_key.split('#').last().unwrap_or(event_key)
    } else {
        event_key
    };
    
    tracing::info!("菜单点击: event_key={}, menu_id={}", event_key, menu_id);
    
    // 检查是否是每日一炼菜单
    let menu_config = get_daily_menu_config();
    if let Some(menu_subject) = menu_config.get(menu_id) {
        // 调用出题 API
        return handle_daily_quiz_generate(&user.sid, &menu_subject.grade, &menu_subject.subject, user.money78 as i32).await;
    }
    
    // 其他菜单处理
    match event_key {
        "daily_quiz" | "每日一炼" => {
            Some(format!("每日一炼功能开发中...\n您的积分: {}", user.money78))
        }
        "my_score" | "我的成绩" => {
            Some(format!(
                "您的信息:\n用户ID: {}\n积分: {}\n消费: {}",
                user.id, user.money78, user.consume
            ))
        }
        key => {
            tracing::info!("Unknown menu key: {}", key);
            Some(format!("未知菜单: {}", key))
        }
    }
}

/// 处理出题请求
async fn handle_daily_quiz_generate(uid: &str, grade: &str, subject: &str, current_score: i32) -> Option<String> {
    use crate::memcached::{get_memcached_client, QuizState};
    
    tracing::info!("开始出题: uid={}, grade={}, subject={}", uid, grade, subject);
    
    // 调用出题 API
    match crate::generate_question(grade, subject, Some(current_score)).await {
        Ok(result) => {
            // 存储到 Memcached
            let quiz_state = QuizState::new(
                subject.to_string(), // idsubject 暂时用科目名
                grade.to_string(),
                subject.to_string(),
                result.question_id.clone(),
                result.question.clone(),
                result.standard_answer.clone(),
                result.explanation.clone(),
                result.score_difficulty,
            );
            
            if let Err(e) = get_memcached_client().set_quiz_state(uid, &quiz_state) {
                tracing::error!("存储答题状态失败: {}", e);
                return Some("出题成功，但存储失败，请重试".to_string());
            }
            
            // 格式化返回消息
            let hint_text = result.hint.map(|h| format!("\n提示：{}", h)).unwrap_or_default();
            Some(format!(
                "【{}{}题】\n{}{}\n\n请直接回复答案",
                grade, subject, result.question, hint_text
            ))
        }
        Err(e) => {
            tracing::error!("出题失败: {}", e);
            Some(format!("出题失败: {}", e))
        }
    }
}

/// 处理判题请求
async fn handle_daily_quiz_judge(wechat_userid: &str, user_answer: &str) -> Option<String> {
    use crate::memcached::get_memcached_client;
    
    // 先登录用户获取 uid
    let user = login_user(wechat_userid).await.ok()?;
    let uid = &user.sid;
    
    // 获取答题状态
    let quiz_state = match get_memcached_client().get_quiz_state(uid) {
        Ok(Some(state)) => state,
        Ok(None) => {
            tracing::info!("用户 {} 没有答题状态", uid);
            return None; // 返回 None，让默认处理生效
        }
        Err(e) => {
            tracing::error!("获取答题状态失败: {}", e);
            return Some("系统错误，请重试".to_string());
        }
    };
    
    // 检查是否过期
    if quiz_state.is_expired(300) {
        let _ = get_memcached_client().delete_quiz_state(uid);
        return Some("答题已超时，请重新选择科目".to_string());
    }
    
    tracing::info!("开始判题: uid={}, answer={}", uid, user_answer);
    
    // 获取用户当前能力分
    let current_score = user.money78;
    
    // 调用判题 API
    match crate::judge_answer(
        uid,
        &quiz_state.grade,
        &quiz_state.subject,
        &quiz_state.question,
        &quiz_state.standard_answer,
        &quiz_state.explanation,
        user_answer,
        current_score as i32,
        quiz_state.score_difficulty,
    ).await {
        Ok(result) => {
            // 删除答题状态
            let _ = get_memcached_client().delete_quiz_state(uid);
            
            // 格式化返回消息
            let correct_text = if result.is_correct { "回答正确！" } else { "回答错误" };
            let score_change_text = if result.score_change > 0 {
                format!("+{}", result.score_change)
            } else {
                result.score_change.to_string()
            };
            
            Some(format!(
                "{} +{}积分\n能力分: {} ({}{})\n\n【解析】\n{}",
                correct_text,
                result.points_change,
                result.new_score,
                score_change_text,
                result.feedback,
                result.explanation
            ))
        }
        Err(e) => {
            tracing::error!("判题失败: {}", e);
            Some(format!("判题失败: {}", e))
        }
    }
}

/// Build text reply XML
pub fn build_text_reply(to_user: &str, from_user: &str, content: &str) -> String {
    format!(
        r#"<xml>
<ToUserName><![CDATA[{}]]></ToUserName>
<FromUserName><![CDATA[{}]]></FromUserName>
<CreateTime>{}</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[{}]]></Content>
</xml>"#,
        to_user,
        from_user,
        chrono::Utc::now().timestamp(),
        content
    )
}

/// Encrypt message for WeWork reply
fn encrypt_message(encoding_aes_key: &str, corp_id: &str, msg: &str) -> Result<String, String> {
    use aes::cipher::{KeyIvInit, BlockEncryptMut, block_padding::Pkcs7};
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::Rng;
    
    // Decode key
    let key = decode_base64_lenient(encoding_aes_key)
        .map_err(|e| format!("Base64 decode key failed: {}", e))?;
    
    // Build plaintext: random(16) + msg_len(4) + msg + corp_id
    let mut rng = rand::thread_rng();
    let random_bytes: Vec<u8> = (0..16).map(|_| rng.gen::<u8>()).collect();
    let msg_bytes = msg.as_bytes();
    let msg_len = msg_bytes.len() as u32;
    let corp_id_bytes = corp_id.as_bytes();
    
    let mut plaintext = Vec::new();
    plaintext.extend_from_slice(&random_bytes);
    plaintext.extend_from_slice(&msg_len.to_be_bytes());
    plaintext.extend_from_slice(msg_bytes);
    plaintext.extend_from_slice(corp_id_bytes);
    
    let pt_len = plaintext.len();
    
    // Add padding space (AES block size is 16)
    let block_size = 16usize;
    let padded_len = ((pt_len / block_size) + 1) * block_size;
    plaintext.resize(padded_len, 0);
    
    // AES-256-CBC encrypt with key's first 16 bytes as IV
    let iv = &key[..16];
    let cipher = Aes256CbcEnc::new_from_slices(&key, iv)
        .map_err(|e| format!("Create cipher failed: {}", e))?;
    
    // Encrypt with PKCS7 padding
    let ciphertext = cipher.encrypt_padded_mut::<Pkcs7>(&mut plaintext, pt_len)
        .map_err(|e| format!("Encrypt failed: {}", e))?;
    
    Ok(STANDARD.encode(ciphertext))
}

/// Build encrypted reply XML
fn build_encrypted_reply(token: &str, encoding_aes_key: &str, corp_id: &str, to_user: &str, from_user: &str, content: &str) -> Result<String, String> {
    use rand::Rng;
    
    let timestamp = chrono::Utc::now().timestamp();
    let nonce: u32 = rand::thread_rng().gen();
    
    // Build inner message
    let inner_msg = format!(
        r#"<xml>
<ToUserName><![CDATA[{}]]></ToUserName>
<FromUserName><![CDATA[{}]]></FromUserName>
<CreateTime>{}</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[{}]]></Content>
</xml>"#,
        to_user, from_user, timestamp, content
    );
    
    // Encrypt message
    let encrypted = encrypt_message(encoding_aes_key, corp_id, &inner_msg)?;
    
    // Generate signature
    let ts_str = timestamp.to_string();
    let nonce_str = nonce.to_string();
    let mut arr = vec![token, &ts_str, &nonce_str, &encrypted];
    arr.sort();
    let combined = arr.join("");
    let mut hasher = Sha1::new();
    hasher.update(combined.as_bytes());
    let msg_signature = format!("{:x}", hasher.finalize());
    
    Ok(format!(
        r#"<xml>
<Encrypt><![CDATA[{}]]></Encrypt>
<MsgSignature><![CDATA[{}]]></MsgSignature>
<TimeStamp>{}</TimeStamp>
<Nonce><![CDATA[{}]]></Nonce>
</xml>"#,
        encrypted, msg_signature, timestamp, nonce
    ))
}

/// Verify signature
/// URL验证时需要包含 echostr，消息接收时不需要
fn verify_signature(token: &str, timestamp: &str, nonce: &str, signature: &str, echostr: Option<&str>) -> bool {
    tracing::info!("verify_signature: token={}, timestamp={}, nonce={}, signature={}, echostr={:?}", token, timestamp, nonce, signature, echostr);
    
    let mut arr = vec![token, timestamp, nonce];
    if let Some(echo) = echostr {
        arr.push(echo);
    }
    arr.sort();
    
    let combined = arr.join("");
    let mut hasher = Sha1::new();
    hasher.update(combined.as_bytes());
    let result = format!("{:x}", hasher.finalize());
    
    tracing::info!("Signature verification: combined={}, calculated={}, expected={}", combined, result, signature);
    
    result == signature
}

/// Decrypt echostr for URL verification
/// WeWork echostr format after decryption: random(16 bytes) + msg_len(4 bytes) + msg + corp_id
fn decrypt_echostr(encoding_aes_key: &str, corp_id: &str, encrypted: &str) -> Result<String, String> {
    // Decode base64 key (43 chars -> 32 bytes)
    // WeWork uses non-standard base64 with invalid trailing bits
    let key_str = encoding_aes_key.trim();
    
    tracing::info!("Decoding key: len={}, key={}", key_str.len(), key_str);
    
    // Use lenient decoder for WeWork's non-standard base64
    let key = decode_base64_lenient(key_str)
        .map_err(|e| format!("Base64 decode key failed: {}", e))?;
    
    tracing::info!("Key decoded: {} bytes", key.len());
    
    // Decode encrypted content
    let encrypted_bytes = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| format!("Base64 decode content failed: {}", e))?;
    
    tracing::debug!("Encrypted content: {} bytes", encrypted_bytes.len());
    
    if encrypted_bytes.len() < 32 {
        return Err("Encrypted content too short".to_string());
    }
    
    // AES-256-CBC decrypt
    // 企业微信: IV = key 的前 16 字节，整个 encrypted 都是密文
    let iv = &key[..16];
    let ciphertext = &encrypted_bytes[..];
    
    tracing::debug!("IV (from key): {:02x?}", iv);
    tracing::debug!("Ciphertext: {} bytes", ciphertext.len());
    
    let cipher = Aes256CbcDec::new_from_slices(&key, iv)
        .map_err(|e| format!("Create cipher failed: {}", e))?;
    
    let mut buf = ciphertext.to_vec();
    let decrypted = cipher.decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("Decrypt failed: {}", e))?;
    
    tracing::debug!("Decrypted: {} bytes", decrypted.len());
    
    // Parse decrypted content: random(16) + msg_len(4) + msg + corp_id
    if decrypted.len() < 20 {
        return Err("Decrypted content too short".to_string());
    }
    
    // Skip 16 bytes random
    let msg_len_bytes = &decrypted[16..20];
    let msg_len = u32::from_be_bytes([msg_len_bytes[0], msg_len_bytes[1], msg_len_bytes[2], msg_len_bytes[3]]) as usize;
    
    tracing::debug!("Msg len: {}", msg_len);
    
    if decrypted.len() < 20 + msg_len {
        return Err(format!("Invalid msg_len: {} vs {}", msg_len, decrypted.len() - 20));
    }
    
    let msg = std::str::from_utf8(&decrypted[20..20+msg_len])
        .map_err(|e| format!("UTF8 decode failed: {}", e))?;
    
    // Verify corp_id (optional)
    let corp_id_start = 20 + msg_len;
    if decrypted.len() > corp_id_start {
        let received_corp_id = std::str::from_utf8(&decrypted[corp_id_start..])
            .unwrap_or("");
        if !received_corp_id.is_empty() && received_corp_id != corp_id {
            tracing::warn!("CorpID mismatch: expected {}, got {}", corp_id, received_corp_id);
        }
    }
    
    Ok(msg.to_string())
}

/// Decrypt message (AES-256-CBC)
fn decrypt_message(encoding_aes_key: &str, encrypted: &str) -> Result<String, String> {
    // Decode base64 key - WeWork uses non-standard base64 with invalid trailing bits
    let key = decode_base64_lenient(encoding_aes_key)
        .map_err(|e| format!("Base64 decode key failed: {}", e))?;
    
    // Decode encrypted content
    let encrypted_bytes = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| format!("Base64 decode content failed: {}", e))?;
    
    tracing::info!("Decrypt: encrypted len={}, is_16_multiple={}", encrypted_bytes.len(), encrypted_bytes.len() % 16 == 0);
    
    if encrypted_bytes.len() < 32 {
        return Err("Encrypted content too short".to_string());
    }
    
    // AES-256-CBC decrypt - WeWork uses key's first 16 bytes as IV
    let iv = &key[..16];
    
    let cipher = Aes256CbcDec::new_from_slices(&key, iv)
        .map_err(|e| format!("Create cipher failed: {}", e))?;
    
    let mut buf = encrypted_bytes.to_vec();
    
    // Try PKCS7 first
    let decrypted_result = cipher.decrypt_padded_mut::<Pkcs7>(&mut buf);
    
    let decrypted: Vec<u8> = match decrypted_result {
        Ok(d) => d.to_vec(),
        Err(e) => {
            // Manual decrypt without padding validation
            tracing::info!("PKCS7 failed ({}), trying manual unpad", e);
            let mut buf2 = encrypted_bytes.to_vec();
            let cipher2 = Aes256CbcDec::new_from_slices(&key, iv)
                .map_err(|e| format!("Create cipher failed: {}", e))?;
            cipher2.decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf2)
                .map_err(|e| format!("Decrypt failed: {}", e))?;
            
            // Manual PKCS7 unpad - remove padding bytes
            let pad_len = buf2[buf2.len() - 1] as usize;
            if pad_len > 0 && pad_len <= 16 {
                buf2[..buf2.len() - pad_len].to_vec()
            } else {
                buf2
            }
        }
    };
    
    // Parse: random(16) + msg_len(4) + msg + corp_id
    if decrypted.len() < 20 {
        return Err("Decrypted content too short".to_string());
    }
    
    let msg_len_bytes = &decrypted[16..20];
    let msg_len = u32::from_be_bytes([msg_len_bytes[0], msg_len_bytes[1], msg_len_bytes[2], msg_len_bytes[3]]) as usize;
    
    if decrypted.len() < 20 + msg_len {
        return Err("Invalid msg_len".to_string());
    }
    
    let msg = std::str::from_utf8(&decrypted[20..20+msg_len])
        .map_err(|e| format!("UTF8 decode failed: {}", e))?;
    
    Ok(msg.to_string())
}

/// Decrypt XML message body
fn decrypt_xml_message(encoding_aes_key: &str, corp_id: &str, xml: &str) -> Result<String, String> {
    // Extract Encrypt tag
    let encrypted = extract_encrypt_tag(xml)?;
    decrypt_message(encoding_aes_key, encrypted)
}

/// Extract Encrypt tag from XML
fn extract_encrypt_tag(xml: &str) -> Result<&str, String> {
    // Try CDATA first
    let start = "<Encrypt><![CDATA[";
    let end = "]]></Encrypt>";
    
    if let Some(s) = xml.find(start) {
        let start_idx = s + start.len();
        if let Some(e) = xml[start_idx..].find(end) {
            return Ok(&xml[start_idx..start_idx+e]);
        }
    }
    
    // Try without CDATA
    let start = "<Encrypt>";
    let end = "</Encrypt>";
    if let Some(s) = xml.find(start) {
        let start_idx = s + start.len();
        if let Some(e) = xml[start_idx..].find(end) {
            return Ok(&xml[start_idx..start_idx+e]);
        }
    }
    
    Err("Encrypt tag not found".to_string())
}
