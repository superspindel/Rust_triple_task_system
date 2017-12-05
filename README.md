# `rust_lab5`

## `Build`
* `git clone https://github.com/superspindel/rust_triple_task_system.git`
* `cd rust_triple_task_system`
* `xargo build`

## `Run`
* First terminal window run `openocd -f interface/stlink-v2-1.cfg -f target/stm32f4x.cfg`
* Second terminal window run `itmdump /tmp/itm.fifo`
* Third terminal window run `screen /dev/XXXX 115200` where XXXX is the USB port that is connected to the board, example    `tty.usbmodem1413`.
* Fourth terminal window run `arm-none-eabi-gdb target/thumbv7em-none-eabihf/debug/app`

## `Communicate`
* Second terminal window will print CPU usage of device
* Third terminal is where you can send commands that handle the led lamp: `pause`, `start` and `period XXX` where XXX is the selected period, example `1000`is 1000ms.
