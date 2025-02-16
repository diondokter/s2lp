# Rust driver for the S2-LP

Parts of this driver have been translated from/inspired by the official ST driver.

## Features:

Operations:
- [x] Chip init
- [x] Send
- [x] Receive
- [ ] Tx power config
- [ ] Rx sensitivity config
- [x] Gpio config
- [x] Standby

Packet formats:
- [x] Basic packet format
- [ ] STack packet format
- [x] IEEE 802.15.4g packet format
- [ ] Uart over air packet format
- [ ] MBus packet format (?? Not a real packet format, but a combination of settings)

Radio:
- [x] (G)FSK
- [ ] ASK/OOK

Packet handler engine:
- [ ] Payload transmission order
- [x] Automatic packet filtering
- [ ] Data coding and integrity check
- [x] CRC

Link layer protocol:
- [ ] Auto acknowledgement
  - [ ] Automatic acknowledgment with piggybacking
  - [ ] Automatic retransmission
- [ ] Timeout protocol engine
  - [x] RX Timer
  - [ ] LDC Timer
  - [ ] Sniff Timer
- [x] CSMA/CA

Low level:
- [x] Register definitions