# Verify - SID 验证模块

## 概述

提供会话验证功能，使用 LoversDataState。

## 公开方法

1. `get_lovers_state()` - 获取默认的 LoversDataState

## 测试方案

### 主要逻辑测试

1. **test_get_lovers_state**：验证获取 LoversDataState
   - 验证返回的实例可用

### 其它测试（边界、异常等）

2. **test_lovers_state_table_name**：验证表名正确
