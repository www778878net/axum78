# Context - 请求上下文

## 概述

提供请求上下文相关类型：
- UpInfo：请求上下文（重导出 base::UpInfo）
- RequestBody：请求体格式

## 公开类型

### UpInfo
重导出 `base::UpInfo`，测试由 base crate 负责。

### RequestBody
请求体格式，对应 logsvc POST 请求体。

## 类结构

### RequestBody
- `sid`: String - 会话ID
- `pars`: Vec<Value> - 参数数组
- `cols`: Vec<String> - 列名数组
- `mid`: String - 消息ID
- `midpk`: Option<i64> - 消息主键
- `order`: String - 排序
- `start`: Option<i64> - 起始位置
- `number`: Option<i64> - 数量

## 公开方法

1. `from_json(json)` - 从 JSON 字符串解析
2. `default()` - 默认值

## 测试方案

### 主要逻辑测试

1. **test_request_body_default**：验证默认值
   - 所有字段使用 #[serde(default)]

2. **test_request_body_from_json_valid**：从有效 JSON 解析
   - 验证字段正确解析

3. **test_request_body_from_json_partial**：部分字段 JSON
   - 缺失字段使用默认值

### 其它测试（边界、异常等）

4. **test_request_body_from_json_empty**：空 JSON 对象
5. **test_request_body_from_json_invalid**：无效 JSON
6. **test_request_body_from_json_with_array_pars**：pars 包含复杂类型
   - 验证 pars 字段可以包含对象和数组

### 重导出类型说明

- **UpInfo**：重导出自 base crate，测试由 base crate 负责
