#!/bin/bash

if [[ "$EUID" -eq 0 ]]; then
    echo "This script is not meant to be run as root. Please run as a user with sudo privileges."
    exit 1
fi;
sudo -k
if ! sudo true; then
    echo "This script requires sudo privileges to run."
    exit 1
fi;
sudo ping -c 1 1.1.1.1 > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "This script requires an internet connection to work."
    exit 1
fi;
echo "------------------------------------------------"
echo "-------- Now uninstalling Chronoportal! --------"
echo "------------------------------------------------"
echo "----------- Stopping portal service. -----------"
echo "------------------------------------------------"
sudo systemctl stop portal
echo "----------- Disabling portal service -----------"
echo "------------------------------------------------"
sudo systemctl disable portal
if [[ -e /etc/systemd/system/portal.service ]]; then
    echo "----------- Removing portal service. -----------"
    echo "------------------------------------------------"
    sudo rm /etc/systemd/system/portal.service
fi;
if [[ -e /etc/systemd/system/portal-quit.service ]]; then
    echo "-------- Removing portal quit service. ---------"
    echo "------------------------------------------------"
    sudo rm /etc/systemd/system/portal-quit.service
fi;
export USER=$(whoami)
if [[ -e /home/$USER/portal ]]; then
    echo "------------ Removing portal repo --------------"
    echo "------------------------------------------------"
    rm -rf /home/$USER/portal
fi;
if [[ -e /home/$USER/portal-quit ]]; then
    echo "---------- Removing portal quit repo -----------"
    echo "------------------------------------------------"
    rm -rf /home/$USER/portal-quit
fi;
echo "------ Removing git directories as safe. -------"
echo "------------------------------------------------"
git config --global --unset safe.direcotry /home/$USER/portal
git config --global --unset safe.direcotry /home/$USER/portal-quit
if [[ -e /portal/ ]]; then
    echo "---------- Removing portal directory. ----------"
    echo "------------------------------------------------"
    sudo rm -rf /portal/
fi;
rustup -V > /dev/null 2> /dev/null
if [[ $? == 0 ]]; then
    echo "-------------- Uninstalling rust. --------------"
    echo "------------------------------------------------"
    rustup self uninstall
fi;
sudo apt list --installed 2>> /dev/null | grep libssl-dev > /dev/null 2> /dev/null
if [[ $? == 0 ]]; then
    echo "------ Uninstalling libssl-dev (OpenSSL). ------"
    echo "------------------------------------------------"
    sudo apt remove libssl-dev -y
fi;
sudo apt list --installed 2>> /dev/null | grep libasound2-dev > /dev/null 2> /dev/null
if [[ $? == 0 ]]; then
    echo "--------- Uninstalling libasound2-dev ----------"
    echo "------------------------------------------------"
    sudo apt remove libasound2-dev -y
fi;
sudo apt list --installed 2>> /dev/null | grep pkg-config > /dev/null 2> /dev/null
if [[ $? == 0 ]]; then
    echo "----------- Uninstalling pkg-config. -----------"
    echo "------------------------------------------------"
    sudo apt remove pkg-config -y
fi;
cc -v > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "-------------- Uninstalling gcc. ---------------"
    echo "------------------------------------------------"
    sudo apt remove gcc -y
fi;
echo "---------- Autoremoving apt packages -----------"
echo "------------------------------------------------"
sudo apt autoremove
if [[ -e /etc/sudoers.d/chronoportal ]]; then
    echo "------------ Removing nopasswd sudo ------------"
    echo "------------------------------------------------"
    sudo rm /etc/sudoers.d/chronoportal
fi;
echo "------------ Uninstall is finished! ------------"
echo "------------------------------------------------"