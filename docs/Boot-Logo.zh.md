# 导入开机图片

`tools/flash_boot_logo.py` 会把一张图片转换成128×64单色位图，并烧录成电台的自定义开机画面。

> [!NOTE]
> 这个操作是在电台**正常开机、运行Aura固件**状态下进行的，不是在`UPDATE`引导模式下。写入过程中电台屏幕会切换到`AURA CPS PROGRAMMING...`界面，写完后会自动重启回到正常界面。

## 准备工作

- 安装好[`pyserial`](https://pypi.org/project/pyserial/)和[`Pillow`](https://pypi.org/project/pillow/)的Python 3环境：
  ```sh
  pip install pyserial pillow
  ```
- 和刷机时用的同一根USB转TTL写频线。
- 一张源图片。由于屏幕是128×64纯黑白（没有灰度），简单的高对比度线稿或文字logo效果会比照片好得多。

## 用法

```sh
python tools/flash_boot_logo.py <串口> <图片路径>
```

例如：

```sh
python tools/flash_boot_logo.py /dev/ttyUSB0 logo.png
```

Windows下把`/dev/ttyUSB0`换成对应的COM口（例如`COM3`）。

默认情况下，工具会在写入后读回一遍数据来确认写入是否正确。

### 参数说明

| 参数 | 说明 |
|---|---|
| `--threshold 0-255` | 用简单的亮度阈值来转黑白，而不是用抖动（dithering）算法。线稿或logo建议试试`128`——不加这个参数时默认用抖动，抖动在照片上效果更好，但在简单图形上会显得很花。 |
| `--stretch` | 把图片直接拉伸铺满128×64（不保持长宽比）。默认行为是保持长宽比居中缩放。 |
| `--invert` | 转换后把黑白反过来。 |
| `--preview FILE.png` | 保存实际会发送的128×64黑白图片，方便烧录前先确认效果对不对。 |
| `--baud BAUD` | 串口波特率，默认`115200`，一般不需要修改。 |
| `--no-verify` | 跳过写入后的读回校验（默认会校验）。 |

### 烧录前先预览

因为屏幕没有灰度，建议先看一眼转换后的效果再花时间上传：

```sh
python tools/flash_boot_logo.py /dev/ttyUSB0 logo.png --threshold 128 --preview preview.png
```

打开`preview.png`，确认在128×64下依然清晰可辨——如果不满意，可以试试`--invert`、调整`--threshold`，或者换一张对比度更干净的原图。

## 让开机画面真正显示出来

光上传位图并不会让电台自动切换到显示它——这是电台上一个独立的设置项，需要另外手动开启：

1. 在电台上进入设置菜单，找到**`SYSTEM -> BOOTSCR`**。
2. 把它从`NONE`/`VOLT`/`MSG`改成**`LOGO`**。

重新开机就能看到效果了。
