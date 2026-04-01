# AuthMiddleware - SID 验证中间件

## 概述

提供路由层统一验证功能。

## 类结构

### AuthConfig
认证配置，包含三级白名单：
- `skip_apisys`: HashSet<String> - 一级白名单：apisys
- `skip_apimicro`: HashSet<String> - 二级白名单：apisys/apimicro
- `skip_routes`: HashSet<String> - 三级白名单：apisys/apimicro/apiobj

### MinimalRequest
最小请求体，用于解析客户端请求。

## 公开方法

1. `AuthConfig::load()` - 从配置文件加载
2. `AuthConfig::should_skip(apisys, apimicro, apiobj)` - 检查是否跳过验证
3. `get_auth_config()` - 获取认证配置
4. `sid_auth_middleware()` - SID 验证中间件

## 白名单规则

- 一级：apisys（如 apiguest）
- 二级：apisys/apimicro（如 apitest/test）
- 三级：apisys/apimicro/apiobj（如 apiuser/user/login）

## 测试方案

### 主要逻辑测试

1. **test_auth_config_default**：验证 AuthConfig 默认值
2. **test_auth_config_should_skip_apisys**：一级白名单匹配
3. **test_auth_config_should_skip_apimicro**：二级白名单匹配
4. **test_auth_config_should_skip_routes**：三级白名单匹配
5. **test_auth_config_should_not_skip**：不在白名单

### 其它测试（边界、异常等）

6. **test_minimal_request_default**：验证 MinimalRequest 默认值
7. **test_minimal_request_to_upinfo**：转换为 UpInfo
8. **test_minimal_request_json_deserialize**：从 JSON 反序列化
9. **test_minimal_request_json_partial**：部分字段 JSON
