# Muxa privacy policy

Effective 30 October 2026. This policy covers the Muxa apps for iPhone, iPad and Android.

**Muxa collects no data.** It has no server and no account. It does not track you, show ads, or send analytics or crash reports. The developer receives nothing from the app.

## What stays on your device

- **Your hosts:** each host's name, address, user name, and the host key you trusted. They are stored in the app's own storage.
- **The app's SSH key:** made on the device the first time you connect.
  - On iPhone and iPad, the key is in the Keychain, readable only while the device is unlocked, and never copied to another device.
  - On Android, it is sealed with a key in the Android Keystore that works only while the device is unlocked.
- **Backups:**
  - On Android, the app opts out of backups and device transfer, so nothing is copied.
  - On iPhone and iPad, the host list is part of your device's own backup, if you make one. The key is not.
- **Your password:** if you use the one-time setup, it is used once to add the app's key to your host. It is never stored.

Deleting the app deletes all of this.

## What goes over the network

Muxa connects only to the hosts you add, over SSH, directly from your device. What you see and type in Muxa travels between your device and your own machine, encrypted by SSH. It does not pass through any server of the developer's.

The Reader shows your documents, changes and images without loading anything from the internet. Links that you tap, such as the setup guide, open in your browser.

## Children

Muxa is a tool for software developers. It is not directed at children.

## Changes and contact

Changes to this policy are published here, and the file's history shows each one. For questions, open an issue: https://github.com/xdgetech/muxa/issues
