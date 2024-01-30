#!/bin/bash

PORTAL_DEST=/portal/
SERVICE_NAME=portal
PORTAL_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal/releases/latest'
QUIT_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal-quit/releases/latest'

# Check OS type & architecture
OS=$(uname)
if [[ $? -ne 0 ]] || [[ $OS -ne "Linux" ]]; then
    echo "This script is only designed to work on linux."
    exit 1
fi;
ARCH=$(uname -m)
if [[ $ARCH -ne "x86_64" ]] && [[ $ARCH -ne "armv7"* ]]; then
    echo "This script does not recognize the system's architecture."
    exit 1
fi;
if [[ $ARCH -eq "x86_64" ]]; then
    TARGET=amd64-linux
else
    TARGET=armv7-linux
fi;

# Check if the main portal software is up to date.
echo Checking latest portal release version.
LATEST_PORTAL=`curl ${PORTAL_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_PORTAL='0.0.0'
if [[ -e ${PORTAL_DEST}version.txt ]]; then
    CURRENT_PORTAL=`cat ${PORTAL_DEST}version.txt | sed -e "s/v//"`
fi;
echo Latest portal version is ${LATEST_PORTAL} - current portal version is ${CURRENT_PORTAL}.
LATEST_PORTAL_VERSION_MAJOR=`echo ${LATEST_PORTAL} | cut -d '.' -f 1`
LATEST_PORTAL_VERSION_MINOR=`echo ${LATEST_PORTAL} | cut -d '.' -f 2`
LATEST_PORTAL_VERSION_PATCH=`echo ${LATEST_PORTAL} | cut -d '.' -f 3`
CURRENT_PORTAL_MAJOR=`echo ${CURRENT_PORTAL} | cut -d '.' -f 1`
CURRENT_PORTAL_MINOR=`echo ${CURRENT_PORTAL} | cut -d '.' -f 2`
CURRENT_PORTAL_PATCH=`echo ${CURRENT_PORTAL} | cut -d '.' -f 3`
# If the latest version has a higher major version, update.
if [[ ${LATEST_PORTAL_VERSION_MAJOR} -gt ${CURRENT_PORTAL_MAJOR} ]] ||
        [[ ${LATEST_PORTAL_VERSION_MAJOR} -eq ${CURRENT_PORTAL_MAJOR} && ${LATEST_PORTAL_VERSION_MINOR} -gt ${CURRENT_PORTAL_MINOR} ]] ||
        [[ ${LATEST_PORTAL_VERSION_MAJOR} -eq ${CURRENT_PORTAL_major} && ${LATEST_PORTAL_VERSION_MINOR} -eq ${CURRENT_PORTAL_MINOR} && ${LATEST_PORTAL_VERSION_PATCH} -gt ${CURRENT_PORTAL_PATCH} ]]; then
        echo New version found! Updating portal now.
        DOWNLOAD_URL=`curl ${PORTAL_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
        curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal.tar.gz
        gunzip ${PORTAL_DEST}release-portal.tar.gz
        tar -xf ${PORTAL_DEST}release-portal.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal.tar
        sudo systemctl restart ${SERVICE_NAME}
        echo Portal update complete.
else
        echo Portal already up to date.
fi

# Check if our quit software is up to date as well.
echo Checking latest portal quit release version.
LATEST_QUIT=`curl ${QUIT_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_QUIT='0.0.0'
if [[ -e ${PORTAL_DEST}quit-version.txt ]]; then
    CURRENT_QUIT=`cat ${PORTAL_DEST}quit-version.txt | sed -e "s/v//"`
fi;
echo Latest portal quit version is ${LATEST_QUIT} - current portal quit version is ${CURRENT_QUIT}.
LATEST_QUIT_VERSION_MAJOR=`echo ${LATEST_QUIT} | cut -d '.' -f 1`
LATEST_QUIT_VERSION_MINOR=`echo ${LATEST_QUIT} | cut -d '.' -f 2`
LATEST_QUIT_VERSION_PATCH=`echo ${LATEST_QUIT} | cut -d '.' -f 3`
CURRENT_QUIT_MAJOR=`echo ${CURRENT_QUIT} | cut -d '.' -f 1`
CURRENT_QUIT_MINOR=`echo ${CURRENT_QUIT} | cut -d '.' -f 2`
CURRENT_QUIT_PATCH=`echo ${CURRENT_QUIT} | cut -d '.' -f 3`
# If the latest version has a higher major version, update.
if [[ ${LATEST_QUIT_VERSION_MAJOR} -gt ${CURRENT_QUIT_MAJOR} ]] ||
        [[ ${LATEST_QUIT_VERSION_MAJOR} -eq ${CURRENT_QUIT_MAJOR} && ${LATEST_QUIT_VERSION_MINOR} -gt ${CURRENT_QUIT_MINOR} ]] ||
        [[ ${LATEST_QUIT_VERSION_MAJOR} -eq ${CURRENT_QUIT_major} && ${LATEST_QUIT_VERSION_MINOR} -eq ${CURRENT_QUIT_MINOR} && ${LATEST_QUIT_VERSION_PATCH} -gt ${CURRENT_QUIT_PATCH} ]]; then
        echo New version found! Updating portal quit now.
        DOWNLOAD_URL=`curl ${QUIT_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
        curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal-quit.tar.gz
        gunzip ${PORTAL_DEST}release-portal-quit.tar.gz
        tar -xf ${PORTAL_DEST}release-portal-quit.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal-quit.tar
        echo Portal quit update complete.
else
        echo Portal quit already up to date.
fi