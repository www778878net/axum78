//! testtb - 示范：如何给表加自定义函数
//!
//! 路径: apitest/testmenu/testtb
//! 路由: POST /apitest/testmenu/testtb/:apifun
//!
//! 对应 NodeJS: class testtb extends CidBase78 { async my_fn() {} }
//!
//! 两种写法：
//!   方式一（空壳）：直接用 MysqlCidBase78，拥有 get，不需要写任何方法
//!   方式二（包装）：包一层 struct 实现 Controller78，加自定义函数，未匹配的 fallback 给基类

use axum::http::Method;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{Controller78, async_trait, MysqlCidBase78, Mysql78};

// ============================================================
// 方式一：空壳（注释掉，下面用方式二示范）
// ============================================================
// #[cfg(feature = "datasync_mysql")]
// pub fn register_controller() {
//     let mysql = crate::apisvc::backsvc::datasync_mysql::get_mysql_connection()
//         .expect("MySQL 连接失败");
//     crate::router::registry::register(
//         "apitest/testmenu/testtb",
//         Arc::new(MysqlCidBase78::new("testtb", mysql, vec!["kind".to_string(), "item".to_string(), "data".to_string()])),
//     );
// }

// ============================================================
// 方式二：包装 struct，加自定义函数
// ============================================================

#[cfg(feature = "datasync_mysql")]
pub fn register_controller() {
    let mysql = crate::apisvc::backsvc::datasync_mysql::get_mysql_connection()
        .expect("MySQL 连接失败");
    crate::router::registry::register(
        "apitest/testmenu/testtb",
        Arc::new(Testtb::new(mysql)),
    );
}

/// Testtb 包装 MysqlCidBase78，添加自定义业务方法
pub struct Testtb {
    base: MysqlCidBase78,
}

impl Testtb {
    pub fn new(mysql: Arc<Mysql78>) -> Self {
        Self {
            base: MysqlCidBase78::new("testtb", mysql, vec!["kind".to_string(), "item".to_string(), "data".to_string()]),
        }
    }

    // ========== 自定义业务方法 ==========

    /// test —— 简单 echo，前端调用 POST /apitest/testmenu/testtb/test
    pub async fn test(&self, up: &crate::UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        let mut map = HashMap::new();
        map.insert("message".to_string(), Value::String("testtb ok".to_string()));
        map.insert("sid".to_string(), Value::String(up.sid.clone()));
        Ok(vec![map])
    }

    /// count —— 统计该用户记录数，前端调用 POST /apitest/testmenu/testtb/count
    pub async fn count(&self, up: &crate::UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        let sql = format!("SELECT COUNT(*) AS total FROM `testtb` WHERE `cid` = ?");
        self.base.do_get(&sql, vec![Value::String(up.cid.clone())]).await
    }

    /// list_by_date —— 自定义条件查询
    /// 前端调用 POST /apitest/testmenu/testtb/list_by_date
    /// body: { cid, date: "2025-08-05" }
    pub async fn list_by_date(&self, up: &crate::UpInfo) -> Result<Vec<HashMap<String, Value>>, String> {
        let jsdata: Value = serde_json::from_str(
            up.jsdata.as_deref().unwrap_or("{}")
        ).unwrap_or(Value::Null);
        let date = jsdata.get("date").and_then(|v| v.as_str()).unwrap_or("");
        let sql = format!(
            "SELECT * FROM `testtb` WHERE `cid` = ? AND `uptime` LIKE ? ORDER BY `idpk` DESC"
        );
        self.base.do_get(&sql, vec![
            Value::String(up.cid.clone()),
            Value::String(format!("{}%", date)),
        ]).await
    }
}

// ========== Controller78 trait 实现 ==========

#[async_trait]
impl Controller78 for Testtb {
    async fn call(&self, up: &mut crate::UpInfo, fun: &str, method: &Method) -> Value {
        let up_clone = up.clone();

        // 先匹配自定义函数
        let result: Result<Vec<HashMap<String, Value>>, String> = match fun {
            "test" => self.test(&up_clone).await,
            "count" => self.count(&up_clone).await,
            "list_by_date" => self.list_by_date(&up_clone).await,
            // 没匹配到 → fallback 给基类（get）
            _ => return self.base.call(up, fun, method).await,
        };

        // 统一返回
        match result {
            Ok(rows) => {
                let arr: Vec<Value> = rows.iter()
                    .map(|row| serde_json::to_value(row).unwrap_or(Value::Null))
                    .collect();
                Value::Array(arr)
            }
            Err(e) => {
                up.res = -1;
                up.errmsg = e;
                Value::Null
            }
        }
    }
}
