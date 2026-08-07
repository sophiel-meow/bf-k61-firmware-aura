# 导入SSTV图片

`tools/sstv_upload.py` 用于把一张静态图片导入电台的SSTV图片槽位，之后可以用SSTV TX CQ/QSO功能把它发送出去。

> [!NOTE]
> 这个操作是在电台**正常开机、运行Aura固件**状态下进行的，不是在`UPDATE`引导模式下。写入过程中电台屏幕会切换到`AURA CPS PROGRAMMING...`界面，写完后会自动重启回到正常界面。

## 准备工作

- 安装好[`pyserial`](https://pypi.org/project/pyserial/)和[`Pillow`](https://pypi.org/project/pillow/)的Python 3环境：
  ```sh
  pip install pyserial pillow
  ```
- 和刷机时用的同一根USB转TTL写频线。
- 任意Pillow能打开的图片文件（JPEG、PNG等），也就是你想通过SSTV发送的图片。

## 用法

```sh
python tools/sstv_upload.py <串口> <图片路径>
```

例如：

```sh
python tools/sstv_upload.py /dev/ttyUSB0 photo.jpg
```

Windows下把`/dev/ttyUSB0`换成对应的COM口（例如`COM3`）。

上传过程中，电台屏幕会切换到`AURA CPS PROGRAMMING...`界面，这是正常现象；传输完成后电台会自动重启，回到正常界面，不需要手动操作。

### 参数说明

| 参数 | 说明 |
|---|---|
| `--baud BAUD` | 串口波特率，默认`115200`，一般不需要修改。 |
| `--stretch` | 把图片直接拉伸铺满320×240画面（不保持长宽比）。默认行为是保持长宽比居中缩放，多余部分留黑边。 |
| `--preview FILE.png` | 额外保存一张PNG，展示实际会被发送出去的画面效果（经过和电台一样的Y/色度编码来回转换），方便在正式上传前先确认效果对不对。 |

### 上传前先预览

因为SSTV图片存储时色度分量的分辨率是被压缩过的（亮度Y是完整的320×240，色度做了下采样），建议先预览一下实际发送效果：

```sh
python tools/sstv_upload.py /dev/ttyUSB0 photo.jpg --preview preview.png
```

这会生成`preview.png`，可以在上传前后对比看一下效果，如果不满意就取消，调整原图后再传。

## 上传完成后

上传完成、电台自动重启后，图片就已经存进flash里了，可以用电台上的SSTV TX CQ/QSO功能，以Robot 36彩色SSTV格式把它发送出去。
