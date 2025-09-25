#!/bin/bash

DEST=/portal/
SERVICE_NAME=portal
QUIT_SERVICE_NAME=portal-quit

VERSION=2

help_function()
{
    echo ""
    echo "Usage: $0 [-f]"
    echo -e "\t-f\tForces the removal of apt packages."
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

sudo ping -c 1 1.1.1.1 > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "This script requires an internet connection to work."
    exit 1
fi;

FULL_UNINSTALL=false
while getopts "f" opt
do
    case "$opt" in
        f) FULL_UNINSTALL=true ;;
        \?) help_function ;;
    esac
done

echo "------------------------------------------------"
echo "--------- Now uninstalling Chronoportal --------"
echo "------------------------------------------------"
echo "------------ Stopping portal service -----------"
echo "------------------------------------------------"
sudo systemctl stop ${SERVICE_NAME}
echo "----------- Disabling portal service -----------"
echo "------------------------------------------------"
sudo systemctl disable ${SERVICE_NAME}
if [[ -e /etc/systemd/system/${SERVICE_NAME}.service ]]; then
    echo "------------ Removing portal service -----------"
    echo "------------------------------------------------"
    sudo rm /etc/systemd/system/${SERVICE_NAME}.service
fi;
echo "--------- Stopping portal quit service ---------"
echo "------------------------------------------------"
sudo systemctl stop ${QUIT_SERVICE_NAME}
echo "--------- Disabling portal quit service --------"
echo "------------------------------------------------"
sudo systemctl disable ${QUIT_SERVICE_NAME}
if [[ -e /etc/systemd/system/${QUIT_SERVICE_NAME}.service ]]; then
    echo "--------- Removing portal quit service ---------"
    echo "------------------------------------------------"
    sudo rm /etc/systemd/system/${QUIT_SERVICE_NAME}.service
fi;
echo "---------- Reloading systemctl daemons ---------"
echo "------------------------------------------------"
sudo systemctl daemon-reload
if [[ -e ${DEST} ]]; then
    echo "----------- Removing portal directory ----------"
    echo "------------------------------------------------"
    sudo rm -rf ${DEST}
fi;
if [[ -e /etc/sudoers.d/chronoportal ]]; then
    echo "------------ Removing nopasswd sudo ------------"
    echo "------------------------------------------------"
    sudo rm /etc/sudoers.d/chronoportal
fi;
if [[ $FULL_UNINSTALL == true ]]; then
    echo "------------- Removing apt packages ------------"
    echo "------------------------------------------------"
    sudo apt remove curl alsa-utils moreutils -y
fi;
echo "-------------- Uninstall complete --------------"
echo "------------------------------------------------"