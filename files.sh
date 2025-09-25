#! /bin/bash

DEST=/portal/
SERVICE_NAME=portal
QUIT_SERVICE_NAME=portal-quit
UPDATE_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/update.sh'
UNINSTALL_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/uninstall.sh'

VERSION=1

help_function()
{
    echo ""
    echo "Usage: $0 [-f]"
    echo -e "\t-f\tForces the creation of all scripts, service files, and sudoers file."
    exit 1
}

if [[ "$EUID" -eq 0 ]]; then
    echo "This script is not meant to be run as root. Please run as a user with sudo privileges."
    exit 1
fi;
sudo -k
if ! sudo true; then
    echo "This script requires sudo privileges to run."
    exit 1
fi;

FORCE_CREATE=false
while getopts "f" opt
do
    case "$opt" in
        f) FORCE_CREATE=true ;;
        \?) help_function ;;
    esac
done

if ! [[ -e ${DEST}run.sh ]] || [[ $FORCE_CREATE == true ]]; then
    echo "---------- Creating portal run script ----------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" | sudo tee ${DEST}run.sh > /dev/null 2>&1
    echo | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_UPDATE_SCRIPT=\"${DEST}update.sh\"" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_DATABASE_PATH=\"${DEST}chronokeep-portal.sqlite\"" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_SCREEN_BUS=1" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_LEFT_BUTTON=11" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_UP_BUTTON=5" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_DOWN_BUTTON=6" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_RIGHT_BUTTON=13" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_ENTER_BUTTON=26" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "export PORTAL_ZEBRA_SHIFT=True" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "now=\`date +%Y-%m-%d\`" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    echo "${DEST}chronokeep-portal \$1 | ts '[%Y-%m-%d %H:%M:%S]' >> ${DEST}logs/\${now}-portal.log 2>&1" | sudo tee -a ${DEST}run.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}run.sh
    sudo chmod +x ${DEST}run.sh
fi;

if ! [[ -e ${DEST}quit.sh ]] || [[ $FORCE_CREATE == true ]]; then
    echo "---------- Creating portal quit script ---------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" | sudo tee ${DEST}quit.sh > /dev/null 2>&1
    echo | sudo tee -a ${DEST}quit.sh > /dev/null 2>&1
    echo "${DEST}chronokeep-portal-quit >> ${DEST}quit.log" | sudo tee -a ${DEST}quit.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}quit.sh
    sudo chmod +x ${DEST}quit.sh
fi;

echo "------------ Checking update script ------------"
echo "------------------------------------------------"
if ! [[ -e ${DEST}update.sh ]] || [[ $FORCE_CREATE == true ]]; then
    echo "------------ Fetching update script ------------"
    echo "------------------------------------------------"
    curl -L ${UPDATE_SCRIPT_URL} -o ${DEST}update.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}update.sh
    sudo chmod +x ${DEST}update.sh
fi;

echo "---------- Checking uninstall script -----------"
echo "------------------------------------------------"
if ! [[ -e ${DEST}uninstall.sh ]] || [[ $FORCE_CREATE == true ]]; then
    echo "---------- Fetching uninstall script -----------"
    echo "------------------------------------------------"
    curl -L ${UNINSTALL_SCRIPT_URL} -o ${DEST}uninstall.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}uninstall.sh
    sudo chmod +x ${DEST}uninstall.sh
fi;

if ! [[ -e /etc/systemd/system/${SERVICE_NAME}.service ]] || [[ $FORCE_CREATE == true ]]; then
    echo "------------ Creating portal service -----------"
    echo "------------------------------------------------"
    sudo echo "    [Unit]" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "Description=Chronokeep Portal Service" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "Wants=network-online.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "After=network.target network-online.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "StartLimitIntervalSec=0" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "Type=simple" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "Restart=on-failure" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "RestartSec=1" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "User=$USER" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "ExecStart=${DEST}run.sh" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "ExecStop=${DEST}quit.sh" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "ExecRestart=${DEST}run.sh -q" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "WantedBy=multi-user.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service > /dev/null 2>&1
fi;

if ! [[ -e /etc/systemd/system/${QUIT_SERVICE_NAME}.service ]] || [[ $FORCE_CREATE == true ]]; then
    echo "--------- Creating portal quit service ---------"
    echo "------------------------------------------------"
    sudo echo "[Unit]" | sudo tee /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "Description=Ensure Chronokeep Portal closes before a server shutdown occurs." | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "DefaultDependencies=no" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "Before=shutdown.target" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo  | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "Type=oneshot" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "ExecStart=${DEST}quit.sh" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "TimeoutStartSec=0" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo  | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
    sudo echo "WantedBy=shutdown.target" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service > /dev/null 2>&1
fi;