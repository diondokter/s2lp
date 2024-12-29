# Rust driver for the S2-LP

## Features:

Operations:
- [x] Chip init
- [x] Send
- [x] Receive
- [ ] Tx power config
- [x] Gpio config
- [ ] Sleep

Packet formats:
- [x] Basic packet format
- [ ] STack packet format
- [ ] IEEE 802.15.4 packet format
- [ ] Uart over air packet format
- [ ] MBus packet format (?? Not a real packet format, but a combination of settings)

Packet handler engine:
- [ ] Payload transmission order
- [x] Automatic packet filtering
- [ ] Data coding and integrity check
- [x] CRC
- [ ] Data whitening

Link layer protocol:
- [ ] Auto acknowledgement
  - [ ] Automatic acknowledgment with piggybacking
  - [ ] Automatic retransmission
- [ ] Timeout protocol engine
  - [x] RX Timer
  - [ ] LDC Timer
  - [ ] Sniff Timer
- [ ] CSMA/CA

Low level:
- [x] Register definitions