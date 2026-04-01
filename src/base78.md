# Base78 - 控制器基类

## 概述

Base78 是控制器基类，参考 TypeScript 版本的 Base78，提供：
- 组合 DataState（数据操作 + 审计）
- 处理 UpInfo（请求参数）
- 返回格式化（Response）
- 权限检查
- 通用 CRUD 方法

## 类结构

### Base78
- `tbname`: String - 表名
- `uidcid`: String - 隔离字段（cid 或 uid）
- `datastate`: DataState - 数据状态
- `logger`: MyLogger - 日志
- `isadmin`: bool - 是否为管理员表

### CidBase78
基于 CID 隔离的控制器基类，封装 Base78。

## 公开方法

### Base78
1. `new(tbname, uidcid)` - 创建实例
2. `set_admin()` - 设置为管理员表
3. `check_admin_permission(up)` - 检查管理员权限
4. `validate_params(up, required_count)` - 验证参数数量
5. `validate_required(value, field_name)` - 验证必填字段
6. `validate_range(value, min, max, field_name)` - 验证数字范围
7. `get(up, colp)` - 查询记录
8. `get_all(up)` - 查询所有记录
9. `get_by_id(up, id)` - 根据 ID 查询
10. `m_add(up, record)` - 添加记录
11. `m_update(up, id, record)` - 更新记录
12. `m_del(up, id)` - 删除记录
13. `do_get(sql, params)` - 执行自定义查询

### CidBase78
1. `new(tbname)` - 创建实例（uidcid 默认为 "cid"）
2. `set_admin()` - 设置为管理员表
3. `get_all(up)` - 查询所有记录
4. `get_by_id(up, id)` - 根据 ID 查询
5. `m_add(up, record)` - 添加记录
6. `m_update(up, id, record)` - 更新记录
7. `m_del(up, id)` - 删除记录
8. `do_get(sql, params)` - 执行自定义查询

## 测试方案

### 主要逻辑测试

1. **test_base78_new**：验证 Base78 创建实例
   - 验证 tbname 和 uidcid 正确设置
   - 验证 isadmin 默认为 false

2. **test_base78_set_admin**：验证设置为管理员表
   - 验证 isadmin 被设置为 true

3. **test_cid_base78_new**：验证 CidBase78 创建实例
   - 验证 uidcid 默认为 "cid"

4. **test_validate_params_success**：验证参数数量验证成功
   - 提供足够的参数，验证返回 Ok

5. **test_validate_params_insufficient**：验证参数数量不足
   - 提供不足的参数，验证返回 Err

6. **test_validate_required_empty**：验证必填字段为空
   - 空字符串返回 Err

7. **test_validate_required_not_empty**：验证必填字段非空
   - 非空字符串返回 Ok

8. **test_validate_range_valid**：验证数字范围有效
   - 在范围内返回 Ok

9. **test_validate_range_invalid_too_small**：验证数字范围过小
   - 小于最小值返回 Err
10. **test_validate_range_invalid_too_large**：验证数字范围过大
   - 大于最大值返回 Err

### 其它测试（边界、异常等）

11. **test_validate_params_missing_jsdata**：缺少 jsdata 参数
12. **test_validate_params_invalid_json**：jsdata 格式错误
13. **test_check_admin_permission_not_admin**：非管理员表权限检查
