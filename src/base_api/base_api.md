# BaseApi - 基础 CRUD API Trait

## 概述

BaseApi 是所有表 API 的基础 Trait，类似 koa78-base78 的 Base78 类，提供：
- get: 查询
- m_add: 新增 (m 前缀 = 修改操作)
- m_update: 更新
- m_del: 删除

## 类结构

### TableConfig
- `tbname`: String - 表名
- `id_field`: String - 主键字段 (默认 id)
- `idpk_field`: String - 自增主键 (默认 idpk)
- `uidcid`: String - 用户隔离字段 (cid 或 uid)
- `cols`: Vec<String> - 业务字段列表

### BaseApi Trait
- `Entity`: 关联类型，实体类型
- `config()`: 获取表配置
- `db()`: 获取数据库连接
- `context()`: 获取上下文 (UpInfo)
- `create_up_info()`: 创建 UpInfo

## 公开方法

### CRUD 方法
1. `get(params)` - 查询列表，根据 params 构建 WHERE 条件
2. `get_by_id(id)` - 根据 ID 查询单条
3. `m_add(entity)` - 新增记录
4. `m_update(id, entity)` - 更新记录
5. `m_del(id)` - 删除记录

## 测试方案

### 主要逻辑测试

1. **test_table_config_new**：验证 TableConfig 创建
   - 验证各字段正确设置（tbname, id_field, idpk_field, uidcid, cols）

2. **test_table_config_default_fields**：验证默认字段名
   - id_field 默认为 "id"
   - idpk_field 默认为 "idpk"

### 其它测试（边界、异常等）

3. **test_api_error_new**：验证 ApiError 创建
   - 验证 code 和 message 正确设置

4. **test_api_error_display**：验证 ApiError 显示格式
   - 验证 Display trait 输出包含 code 和 message

### 集成测试说明

BaseApi Trait 的 CRUD 方法（get, get_by_id, m_add, m_update, m_del）需要数据库连接支持，应通过具体的实现类进行集成测试：
- 需要实现 BaseApi Trait 的具体类型
- 需要提供 Sqlite78 数据库实例
- 需要创建测试表和数据
- 测试代码应放在具体实现类的测试模块中
