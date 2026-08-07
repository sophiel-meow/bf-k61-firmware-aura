# 导入卫星数据

`tools/update_satellites.py` 用于把卫星过境数据（频率、CTCSS亚音、轨道高度）写入电台，配合卫星过境追踪/多普勒频移自动修正功能使用。

> [!NOTE]
> 这个操作是在电台**正常开机、运行Aura固件**状态下进行的，不是在`UPDATE`引导模式下。写入过程中电台屏幕会切换到`AURA CPS PROGRAMMING...`界面，写完后会自动重启回到正常界面。

## 准备工作

- 安装好[`pyserial`](https://pypi.org/project/pyserial/)的Python 3环境：
  ```sh
  pip install pyserial
  ```
- 和刷机时用的同一根USB转TTL写频线。

## 查看内置卫星列表

脚本自带一份业余卫星列表（中继转发器和SSTV下行卫星）。不需要连接电台就能查看：

```sh
python tools/update_satellites.py --list
```

## 基本用法：写入默认卫星组

不加额外参数时，会写入一组默认的卫星（ISS、SO-50、AO-91、AO-123、CAS-3H、IO-86、PO-101、RS95S、ISS SSTV）：

```sh
python tools/update_satellites.py /dev/ttyUSB0
```

Windows下把`/dev/ttyUSB0`换成对应COM口（例如`COM3`）。

## 自选要写入的卫星

```sh
python tools/update_satellites.py /dev/ttyUSB0 --sat ISS SO-50 "ISS SSTV"
```

或者把脚本认识的所有卫星都写进去（受限于电台的20个槽位上限）：

```sh
python tools/update_satellites.py /dev/ttyUSB0 --all-known
```

### 参数说明

| 参数 | 说明 |
|---|---|
| `--sat 名称 [名称 ...]` | 指定要写入的卫星（合法名称参考`--list`的输出）。不指定时默认写入一组9颗卫星的常用组合。 |
| `--all-known` | 写入所有内置卫星，而不仅是默认组。 |
| `--tle` | 从[Celestrak](https://celestrak.org)在线获取最新TLE数据，为所选卫星计算当前实际轨道高度，而不是用内置的默认高度估计值。 |
| `--tle-file 路径` | 效果同`--tle`，但从本地文件读取TLE数据而不是联网获取（离线环境下，或者你已经有别的来源的TLE文件时很有用）。 |
| `--rx-only` | 清空所有卫星的上行（TX）频率，把所选卫星全部改成仅接收模式。 |
| `--baudrate 波特率` | 串口波特率，默认`115200`，一般不需要修改。 |
| `--dry-run` | 只打印出将会写入的内容，完全不连接电台——适合先核对一下自己选的卫星对不对。 |

### 写入前先核对一遍

正式写入电台之前，建议先跑一次dry-run：

```sh
python tools/update_satellites.py --dry-run --sat ISS SO-50 --tle
```

这会打印出解析后的卫星记录（频率、亚音、轨道高度），但完全不会碰电台。

## 关于轨道高度和TLE数据

每条卫星记录都包含一个轨道高度，用于多普勒频移修正的计算。如果不加`--tle`/`--tle-file`，脚本会用内置表里写死的默认高度——对大多数过境来说已经够用了，但如果你想要更准确的实时轨道高度（尤其是低轨卫星，高度会随时间漂移），可以加`--tle`从Celestrak获取最新数据，或者用`--tle-file`指定自己保存的TLE文件。

## 写入完成后

写入完成、电台自动重启后，你写入的卫星就可以在卫星过境追踪功能里使用了。
