# ASMARA_Rust

*A software ENDEC that listens to broadcast audio streams, decodes EAS/SAME messages, records audio, and pushes rich notifications via Apprise with a real-time monitoring dashboard. Also supports Make Your Own DASDEC, relaying to Icecast, and event-code based filtering.*

> **Inspiration**: This project is inspired by **ASMARA** (Automated System for Monitoring And Relaying Alerts), a now-removed repository by Anastasia Mayer (@A-c0rN on GitHub) with a similar goal. ASMARA_Rust reimagines that pipeline as a native Rust service with a Docker image and a built-in dashboard for monitoring.

---

## Features

- Real-time EAS/SAME message decoding from multiple audio sources (primarily Icecast/Shoutcast streams)
- Audio recording and optional Icecast relaying
- Rich notifications via [Apprise](https://github.com/caronc/apprise) and Discord embed support
- Web-based monitoring dashboard
- [Make Your Own DASDEC](https://github.com/playsamay4/MYOD) support
- Event-code based filtering
- Docker image with everything pre-configured and included
- Highly configurable via JSON
- Modular and extensible architecture
- Written in Rust for performance and safety

---

## Installation, configuration, usage, technical details

[Please refer to the wiki](https://github.com/wagwan-piffting-blud/ASMARA_Rust/wiki) for detailed instructions on installation, configuration, usage, and more that this README cannot cover in-depth.

---

## License

This project is licensed under the **GNU GPL-3.0** (see [`LICENSE`](LICENSE)).

---

## Acknowledgments
- ASMARA project for early inspiration
- SAME decoders and EAS/NWR community research
- Rust ecosystem maintainers

## GenAI Disclosure Notice: Portions of this repository have been generated using Generative AI tools (ChatGPT, ChatGPT Codex, GitHub Copilot).
