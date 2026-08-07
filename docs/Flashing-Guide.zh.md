# 刷机教程

> [!WARNING]
> 原厂UV-K61固件从未公开发布过。一旦刷入Aura，**就无法再刷回原厂固件**——除非将来有人成功从设备中dump出原厂固件。另外，Aura是非官方的固件，可能存在尚未发现的bug，请自行承担刷机风险。

## 准备工作

- 已经配置好的Python 3环境，并安装了[`pyserial`](https://pypi.org/project/pyserial/)。本教程假设你已经完成这一步。
- 一根兼容Quansheng UV-K5/UV-K6系列的USB转TTL写频线——同一根线可以直接用在Aura上。
- 从本仓库的GitHub Releases页面下载 `aura.bin`。

## 第一步：进入UPDATE模式

电台关机状态下，同时按住**侧键1**和**侧键2**，然后打开电源开关。屏幕会显示`UPDATE`，说明已经进入刷机（bootloader）模式。

![UPDATE模式](../assets/flash-update-mode.webp)

## 第二步：连接写频线

将写频线插入电台和电脑之间。

## 第三步：运行刷机命令

```sh
python tools/flash.py aura.bin /dev/ttyUSB0
```

Windows下把`/dev/ttyUSB0`换成写频线对应的串口号（例如`COM3`，不确定的话可以在设备管理器里查看）。

> [!IMPORTANT]
> 刷入过程中不要拔下写频线。
>
> 如果刷机过程中断了也没关系，关机后重复第一步重新进入UPDATE模式，再试一次即可。如果扭电源旋钮也无法关机，直接抠下电池就好，这是安全的，不用担心。

## 第四步：确认刷机成功

如果终端显示`succeed!`，并且设备自动重启，说明刷机成功。

![刷机成功](../assets/flash-succeed.webp)

如果设备**没有**自动重启，说明固件其实没有刷进去——这是一个已知的bug。关机后从第一步重新开始即可。

## 首次开机：FORMAT FLASH?

刷机后首次开机，屏幕会显示`FORMAT FLASH?`。此时按**MENU**会格式化部分SPI flash区域——不会影响已保存的信道和校准数据，但仍然建议先备份一下。

### 备份SPI flash（推荐）

保持在`FORMAT FLASH?`界面：

1. 插入写频线。
2. 运行：
   ```sh
   python tools/dump_aura_spiflash.py /dev/ttyUSB0 backup.bin
   ```
   （Windows下同样把`/dev/ttyUSB0`换成对应串口号）
3. 设备会进入`AURA CPS PROGRAMMING...`界面，读取整颗2MB flash芯片的内容，需要几分钟。如果中途出错，重新运行一次即可。
4. 完成后会生成`backup.bin`文件，请妥善保存。

![备份终端输出](../assets/flash-backup-terminal.webp)

### 完成设置

回到电台，按**MENU**格式化flash，设备会进入主界面。

![主界面](../assets/main.webp)

到这里刷机就完成了！接下来可以继续导入SSTV图片、卫星数据，或者自定义开机画面。
