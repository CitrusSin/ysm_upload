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

> 目前支持的上传后端有：`MCSManager`、`LocalFile`、`Sftp`、`RsyncOverSsh`、`Rsync`。<br>
> 其中 `Sftp`、`RsyncOverSsh` 现在使用 Rust SSH/SFTP 库直接连接远端；`Rsync` 后端暂时还没有可用的纯 Rust 上传实现。

## 首次使用

### 1. 先启动一次服务

如果当前目录没有 `config.yml`，服务首次启动时会自动生成默认配置并退出。<br>
先生成模板，再按实际环境填写。

### 2. 修改 `config.yml`

把 OAuth、RCON、MCSManager 等配置改成你自己的实际值。

### 3. 再次启动服务

完成配置后重新启动即可。默认监听地址是：

```text
http://127.0.0.1:3000
```

## 从源码构建

如果你不是直接使用现成二进制，而是要自己从仓库源码运行，可以按下面步骤准备：

1. 初始化前端子模块：

   ```bash
   git submodule update --init --recursive
   ```

2. 安装 `pnpm`，然后构建前端资源：

   ```bash
   cd frontend
   pnpm install
   pnpm build
   ```

3. 回到项目根目录后构建或运行后端：

   ```bash
   cargo build --release
   # 或
   cargo run
   ```

> `frontend/dist` 会被后端打包为静态资源；如果没有先初始化前端子模块并完成前端构建，Rust 编译阶段会因为缺少这部分文件而失败。

## Docker

服务在容器内会把 `/data` 作为工作目录，并且始终从 `/data/config.yml` 读取配置。<br>
如果配置文件不存在，首次启动会在 `/data` 下生成默认配置并退出。

### 构建镜像

```bash
docker build -t ysm-upload .
```

### 运行容器

```bash
docker run --rm -p 3000:3000 -v ./data:/data ysm-upload
```

首次运行时，容器会在挂载目录里生成 `config.yml` 后退出；修改完成后再次启动即可。

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

首次运行会自动生成一个默认的 `config.yml`。如果你想直接照着改，可以参考下面这个带注释的完整示例：

```yaml
server:
  host: 127.0.0.1 # 服务绑定地址
  port: 3000 # 服务监听端口

oauth:
  prefix_url: http://127.0.0.1:3000 # 对外访问地址，用来拼接 OAuth 回调
  secret_string: your-secret-here-change-this-in-production # Cookie 签名密钥，生产环境必须修改
  providers:
    littleskin:
      provider_type: littleskin # 可选：littleskin / microsoft / blessingskin=<站点地址>
      client_id: your_client_id_here
      client_secret: your_client_secret_here
      scopes:
        - User.Read
        - Player.Read
        - PremiumVerification.Read
      enabled: true
    microsoft:
      provider_type: microsoft
      client_id: your_azure_client_id
      client_secret: your_client_secret_here
      scopes:
        - XboxLive.signin
      enabled: false # 默认示例保留，但默认不启用

rcon:
  host: 127.0.0.1
  port: 25575
  password: your_rcon_password_here

reload_delay: 3s # 执行 ysm model reload 后等待多久再授权，支持 1s / 500ms / 2m

ysm_storage:
  backend: MCSManager # 可选：MCSManager / LocalFile / Sftp / RsyncOverSsh / Rsync
  local:
    upload_dir: /config/yes_steve_model/auth # LocalFile 后端目录
  sftp:
    host: example.com
    port: 22
    username: root
    password: your_ssh_password_here
    remote_dir: /config/yes_steve_model/auth
  rsync_over_ssh:
    host: example.com
    port: 22
    username: root
    password: your_ssh_password_here
    remote_dir: /config/yes_steve_model/auth
  rsync:
    host: example.com
    port: 873
    module: ysm
    remote_dir: auth

mcsmanager:
  enabled: true # 使用默认上传方案时保持启用
  base_url: http://127.0.0.1:23333
  api_key: your_api_key
  daemon_id: your_daemon_id
  instance_id: your_instance_uuid # 兼容旧字段名 instance_uuid
  upload_dir: /config/yes_steve_model/auth
```

- `providers` 下的名字可以自定义，例如 `littleskin`、`microsoft`
- `backend` 决定当前实际使用哪个上传后端
- `LocalFile` 直接写入本地目录
- `Sftp` 通过内置 Rust SSH/SFTP 客户端上传
- `RsyncOverSsh` 目前也通过内置 Rust SSH/SFTP 客户端上传到远端目录，不再依赖本地 `ssh`/`rsync` 命令
- `Rsync` 目标是通过 rsync daemon 上传到 `module/remote_dir`，但当前没有接入不依赖外部命令的上传实现

## 上线前最少需要确认的内容

如果你只是想先把服务用起来，至少要确认下面几项：

1. `oauth.providers` 里至少启用一个可用提供商
2. `rcon` 能正常连接到 Minecraft 服务端
3. `ysm_storage.backend` 选择了你实际可用的一种上传后端
4. 如果使用 `MCSManager`，确认 `mcsmanager.enabled` 为 `true`
5. 如果使用 `MCSManager`，确认 `mcsmanager.base_url`、`api_key`、`daemon_id`、`instance_id` 已正确填写
6. 如果使用 `Sftp` 或 `RsyncOverSsh`，确认对应的 SSH 密码配置已填写
