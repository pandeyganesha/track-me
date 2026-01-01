# Parse Stream
I ran my program and performed following actions to produce info

- Switched different windows, like from vsc to terminal and so on.
- Switched the window to fullscreen and back to normal
- Opened rofi application launcher and opened new program like slack
- Switched to empty screen without any window
- Switched between different workspaces

It produced the following data.
I have removed personal info from the results.

```
focusedmon>><internal_display_name>,1
focusedmonv2>><internal_display_name>,1
activewindow>>kitty,python
activewindowv2>><window_id>
createworkspace>>6
createworkspacev2>>6,6
activewindow>>,
activewindowv2>>
workspace>>6
workspacev2>>6,6
focusedmon>><external_display_name>,2
focusedmonv2>><external_display_name>,2
activewindow>>code,main.rs - track-me - Visual Studio Code
activewindowv2>><window_id>
fullscreen>>1
fullscreen>>0
windowtitle>>5744b4e09970
windowtitlev2>>5744b4e09970,kitty
openwindow>>5744b4e09970,2,kitty,kitty
activewindow>>kitty,kitty
activewindowv2>>5744b4e09970
windowtitle>>5744b4e09970
windowtitlev2>>5744b4e09970,bash
activewindow>>kitty,bash
activewindowv2>>5744b4e09970
windowtitle>>5744b4e09970
windowtitlev2>>5744b4e09970,<username>@archlinux:~
activewindow>>kitty,<username>@archlinux:~
activewindowv2>>5744b4e09970
windowtitle>>5744b4e09970
windowtitlev2>>5744b4e09970,~
activewindow>>kitty,~
activewindowv2>>5744b4e09970
focusedmon>><internal_display_name>,6
focusedmonv2>><internal_display_name>,6
activewindow>>kitty,python
activewindowv2>><window_id>
workspace>>1
workspacev2>>1,1
destroyworkspace>>6
destroyworkspacev2>>6,6
openlayer>>rofi
closelayer>>rofi
activewindow>>kitty,python
activewindowv2>><window_id>
urgent>>5744b4e06c20
focusedmon>><external_display_name>,2
focusedmonv2>><external_display_name>,2
activewindow>>Slack,<User Name> (DM) - <Org name> - Slack
activewindowv2>>5744b4e06c20
workspace>>4
workspacev2>>4,4
activewindow>>Slack,<User Name> (DM) - <Org name> - Slack
activewindowv2>>5744b4e06c20
closewindow>>5744b4e06c20
activewindow>>,
activewindowv2>>
focusedmon>><internal_display_name>,1
focusedmonv2>><internal_display_name>,1
activewindow>>kitty,python
activewindowv2>><window_id>
focusedmon>><external_display_name>,4
focusedmonv2>><external_display_name>,4
activewindow>>kitty,~
activewindowv2>>5744b4e09970
workspace>>2
workspacev2>>2,2
destroyworkspace>>4
destroyworkspacev2>>4,4
activewindow>>kitty,~
activewindowv2>>5744b4e09970
focusedmon>><internal_display_name>,1
focusedmonv2>><internal_display_name>,1
activewindow>>kitty,python
activewindowv2>><window_id>
openlayer>>rofi
closelayer>>rofi
activewindow>>kitty,python
activewindowv2>><window_id>
windowtitle>>5744b4e06c20
windowtitlev2>>5744b4e06c20,<User Name> (DM) - <Org Name> - Slack
openwindow>>5744b4e06c20,1,Slack,<User Name> (DM) - <Org Name> - Slack
activewindow>>Slack,<User Name> (DM) - <Org Name> - Slack
activewindowv2>>5744b4e06c20
closewindow>>5744b4e06c20
activewindow>>kitty,python
activewindowv2>><window_id>
focusedmon>><external_display_name>,2
focusedmonv2>><external_display_name>,2
activewindow>>kitty,~
activewindowv2>>5744b4e09970
activewindow>>code,main.rs - track-me - Visual Studio Code
activewindowv2>><window_id>
```

I found following unique event names

1. focusedmon
1. focusedmonv2
1. activewindow
1. activewindowv2
1. createworkspace
1. createworkspacev2
1. workspace
1. workspacev2
1. fullscreen
1. windowtitle
1. windowtitlev2
1. openwindow
1. destroyworkspace
1. destroyworkspacev2
1. openlayer
1. closelayer
1. urgent
1. closewindow

One thing is clear, the more types of things I do, the more types of events I can notice. Which implies there might be dozen of other events. Like if I move a window from one workspace to another, then it has its own associated event.

## What to store?

Let's first interrogate what we need to store and then decide how to store that.

First question that I have is should I store all the events so that I can have maximum info about my activities. But does it align with my goal?
The goal is to track time expenditure. So according to that i should not be storing all that.

Ultimate goal is not to just track time expenditure but track as many things I can which can help me simply my work and provide insights on how I use my system. So for now, we would filter out all the data we need to track time but later can accept more.

## How to store
We already are getting output in key, value pair sense. Yes, actual data type is string, but presentation of stream seems like mapping where we have a value corresponding to each event.

For now what we want is to note time spent on each process, so we would have a key valye pair (HashMap) which maps process name to time.

For now as we can see, `activewindow` has the data of which window we are focusing on. So we would filter output on it.