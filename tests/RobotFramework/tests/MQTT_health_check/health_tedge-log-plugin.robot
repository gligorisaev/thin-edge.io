#Command to execute:    robot -d \results --timestampoutputs --log health_tedge-log-plugin.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_tedge-log-plugin.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:troubleshooting    theme:monitoring    theme:c8y
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop tedge-log-plugin
    Execute Command    sudo systemctl stop tedge-log-plugin.service

Update the service file
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=30' /lib/systemd/system/tedge-log-plugin.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-log-plugin
    Execute Command    sudo systemctl start tedge-log-plugin.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service
    Sleep    10s

Check PID of tedge-log-plugin
    ${pid}=    Execute Command    pgrep -f '^/usr/bin/tedge-log-plugin'    strip=${True}
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-log-plugin
    ${pid1}=    Execute Command    pgrep -f '^/usr/bin/tedge-log-plugin'        strip=${True}
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '10d' /lib/systemd/system/tedge-log-plugin.service