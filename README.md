# ysm_upload

一个给 **Yes Steve Model（YSM）** 用的上传服务：<br>
前端负责登录和上传模型文件，后端负责 OAuth 登录、校验玩家身份、把模型文件上传到存储后端，并通过 Minecraft RCON 执行 `ysm model reload` 和 `ysm auth <玩家名> add <文件名>`。

## 功能概览

- 支持 OAuth 登录
  - LittleSkin
  - Blessing Skin
  - Microsoft
- 提供前端静态页面
- 登录后可上传 `.ysm` / `.zip` / `.7z` 模型文件
- 校验上传的 `profile_uuid` 是否属于当前登录用户
- 上传完成后自动执行 YSM 重载和授权命令

> 当前真正实现完成的上传后端是 **MCSManager**。<br>
> `LocalFile`、`Sftp`、`RsyncOverSsh`、`Rsync` 配置项已经预留，但代码里还没有实现。

## 运行要求

- Rust（建议使用稳定版）
- `pnpm`
- 一个可用的前端子模块
- 可访问的 OAuth 提供商
- 可访问的 Minecraft RCON
- 如果使用默认上传方案，还需要可访问的 MCSManager 面板 / daemon

## 快速开始

### 1. 拉取仓库和前端子模块

这个项目的前端在 `frontend` 子模块里，首次拉取后要先初始化：

```bash
git clone https://github.com/CitrusSin/ysm_upload.git
cd ysm_upload
git submodule update --init --recursive
```

### 2. 安装前端依赖

```bash
cd frontend
pnpm install
cd ..
```

### 3. 首次启动，生成配置文件

```bash
cargo run
```

如果当前目录没有 `config.yml`，程序会自动生成一个默认配置，然后直接退出。<br>
这一步的目的就是先把配置模板写出来。

### 4. 修改 `config.yml`

把 OAuth、RCON、MCSManager 等配置改成你自己的实际值。

### 5. 再次启动

```bash
cargo run
```

服务默认监听：

```text
http://127.0.0.1:3000
```

## 基础使用方法

### 浏览器使用流程

1. 打开服务首页
2. 选择一个已启用的 OAuth 提供商登录
3. 登录成功后，服务会通过 Cookie 保存登录态
4. 在前端页面选择模型文件并上传
5. 服务会：
   - 校验当前用户拥有目标角色 `profile_uuid`
   - 生成模型文件名
   - 上传到存储后端
   - 连接 RCON 执行模型重载
   - 给对应玩家授权模型

### 接口说明

#### 1. 获取可用登录方式

`GET /api/oauth/providers`

返回当前启用的 OAuth 提供商列表。

#### 2. 发起登录

`GET /api/oauth/{provider}/login`

例如：

```text
/api/oauth/littleskin/login
```

#### 3. 获取当前登录用户

`GET /api/user`

需要已登录。返回当前 OAuth 用户信息和可用角色列表。

#### 4. 上传 YSM 模型

`POST /api/ysm/upload`

需要已登录，请使用 `multipart/form-data`：

- `profile_uuid`: 目标角色 UUID
- `file`: 模型文件，支持 `.ysm` / `.zip` / `.7z`

限制与行为：

- 上传大小上限：`64 MiB`
- 服务会根据文件内容计算一个稳定 ID
- 最终保存文件名格式：`<模型ID>.<原扩展名>`

上传成功后会依次执行：

```text
ysm model reload
ysm auth <profile_name> add <stored_file_name>
```

#### 5. 登出

`GET /api/logout`

## `config.yml` 说明

首次运行会自动生成一个默认的 `config.yml`。下面是各字段的作用。

### `server`

服务监听地址。

```yaml
server:
  host: 127.0.0.1
  port: 3000
```

- `host`: 绑定地址
- `port`: 监听端口

### `oauth`

OAuth 总配置。

```yaml
oauth:
  prefix_url: http://127.0.0.1:3000
  secret_string: change-me
  providers: {}
```

- `prefix_url`: 对外访问这个服务的基础地址，用来拼接回调地址
- `secret_string`: 用来签名登录状态 Cookie，生产环境必须改掉
- `providers`: 多个 OAuth 提供商配置

#### `oauth.providers.<name>`

每个提供商都是一个自定义名字，例如 `littleskin`、`microsoft`。

```yaml
oauth:
  providers:
    littleskin:
      provider_type: littleskin
      client_id: your_client_id_here
      client_secret: your_client_secret_here
      scopes:
        - User.Read
        - Player.Read
        - PremiumVerification.Read
      enabled: true
```

- `provider_type`: 提供商类型
  - `littleskin`
  - `microsoft`
  - `blessingskin=<站点地址>`
- `client_id`: OAuth 应用 ID
- `client_secret`: OAuth 应用密钥
- `scopes`: 申请的权限列表
- `enabled`: 是否启用

### `rcon`

Minecraft 服务器 RCON 配置。

```yaml
rcon:
  host: 127.0.0.1
  port: 25575
  password: your_rcon_password_here
```

- `host`: RCON 地址
- `port`: RCON 端口
- `password`: RCON 密码

### `reload_delay`

```yaml
reload_delay: 3s
```

执行 `ysm model reload` 后，等待多久再执行授权命令。<br>
支持 `1s`、`500ms`、`2m` 这类人类可读时长格式。

### `ysm_storage`

上传后端选择。

```yaml
ysm_storage:
  backend: MCSManager
```

可选值：

- `MCSManager`
- `LocalFile`
- `Sftp`
- `RsyncOverSsh`
- `Rsync`

> 目前只有 `MCSManager` 实际可用。

#### `ysm_storage.local`

```yaml
ysm_storage:
  local:
    upload_dir: /config/yes_steve_model/auth
```

- `upload_dir`: 本地目录

#### `ysm_storage.sftp`

```yaml
ysm_storage:
  sftp:
    host: example.com
    port: 22
    username: root
    remote_dir: /config/yes_steve_model/auth
```

- `host`: SFTP 主机
- `port`: SFTP 端口
- `username`: 登录用户名
- `remote_dir`: 远端目录

#### `ysm_storage.rsync_over_ssh`

```yaml
ysm_storage:
  rsync_over_ssh:
    host: example.com
    port: 22
    username: root
    remote_dir: /config/yes_steve_model/auth
```

- `host`: SSH 主机
- `port`: SSH 端口
- `username`: SSH 用户名
- `remote_dir`: 远端目录

#### `ysm_storage.rsync`

```yaml
ysm_storage:
  rsync:
    host: example.com
    port: 873
    module: ysm
    remote_dir: auth
```

- `host`: rsync 主机
- `port`: rsync 端口
- `module`: rsync module
- `remote_dir`: module 内目录

### `mcsmanager`

默认上传方案使用的 MCSManager 配置。

```yaml
mcsmanager:
  enabled: true
  base_url: http://127.0.0.1:23333
  api_key: your_api_key
  daemon_id: your_daemon_id
  instance_id: your_instance_uuid
  upload_dir: /config/yes_steve_model/auth
```

- `enabled`: 是否启用 MCSManager 上传
- `base_url`: MCSManager 面板地址
- `api_key`: 面板 API Key
- `daemon_id`: 目标 daemon ID
- `instance_id`: 实例 UUID<br>
  兼容旧字段名 `instance_uuid`
- `upload_dir`: 上传目录

## 一个最小可用思路

如果你只是想先跑起来，最少需要保证下面几件事：

1. `frontend` 子模块已经初始化
2. 本机安装了 `pnpm`
3. `oauth.providers` 里至少启用一个可用提供商
4. `rcon` 能正常连接到 Minecraft 服务端
5. `ysm_storage.backend` 使用 `MCSManager`
6. `mcsmanager.enabled` 为 `true`，并且 `base_url`、`api_key`、`daemon_id`、`instance_id` 已填写

## 开发与校验

项目当前 CI 使用的是下面这组命令：

```bash
cd frontend && pnpm install --frozen-lockfile
cd frontend && pnpm build
cargo build --verbose
cargo test --verbose
```

如果没有初始化 `frontend` 子模块，或者没有生成 `frontend/dist`，Rust 编译会失败。
