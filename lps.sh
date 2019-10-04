#!/bin/bash
BASEDIR="$( cd "$( dirname "$0" )" && pwd )"

ACOPY="$BASEDIR/a/lproxy-cv"
BCOPY="$BASEDIR/b/lproxy-cv"
PIDFILE="/var/run/lproxy-cv.pid"

get_copy_version() {
    local path=$1
    if [[ -x $path ]]; then
        echo $($path -v)
    else
        echo '0.0.0'
    fi
}

verlte() {
    [  "$1" = "`echo -e "$1\n$2" | sort -V | head -n1`" ]
}

verlt() {
    [ "$1" = "$2" ] && return 1 || verlte $1 $2
}

get_copy_run() {
    local acopy=$(get_copy_version $ACOPY)
    local bcopy=$(get_copy_version $BCOPY)

    if $(verlt $acopy $bcopy); then
        echo $BCOPY
    else
        echo $ACOPY
    fi
}

start() {
    echo 'start lproxy-cv'
    
    local path=$(get_copy_run)
    $path > /dev/null 2>&1 &
    #$path
}

stop() {
    echo 'stop lproxy-cv'
    if [[ -e $PIDFILE ]]; then
        local pid=$(cat $PIDFILE)
        if [[ -n $pid ]]; then
            kill -s USR1 $pid 
            local count=0
            while [ -e /proc/$pid ]
            do 
                sleep 1
                count=$((count+1))
                if [[ $count -gt 3 ]]; then
                    count=0
                    kill -9 $pid
                fi
            done
        fi
    fi
}

restart() {
    echo 'restart lproxy-cv'
    stop
    start
}

monitor() {
    echo 'monitor lproxy-cv'
    local needcreate=1
    if [[ -e $PIDFILE ]]; then
        local pid=$(cat $PIDFILE)
        if [[ -n $pid ]]; then
            if [[ -e /proc/$pid ]]; then
                $needcreate=0
            fi
        fi
    fi

    if [[ $needcreate -ne 0 ]]; then
        start
    fi
}

operation=$1
case $operation in
    start)
        start;;
    stop)
        stop;;
    restart)
        restart;;
    monitor)
        monitor;;
    *)
        echo 'unknown operation';;
esac

