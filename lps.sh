#!/bin/sh
BASEDIR="$( cd "$( dirname "$0" )" && pwd )"

ACOPY="$BASEDIR/a/lproxy-cv"
BCOPY="$BASEDIR/b/lproxy-cv"
PIDFILE="/var/run/lproxy-cv.pid"
#change for each device
UUID=$(cat /root/lproxy/UUID)
# get the version of this copy
get_copy_version() {
    local  __resultvar=$1
    eval $__resultvar=0
    local path=$2
    if [[ -x "$path" ]]; then
        $path -vn > /dev/null 2>&1
        if [[ $? -eq 0 ]]
        then
            eval $__resultvar=$($path -vn)
        fi
    fi
}

# get current copy to run with
get_copy_run() {
    local  __resultvar=$1
    get_copy_version result $ACOPY
    local acopy=$result
    get_copy_version result $BCOPY
    local bcopy=$result

    if [[ $bcopy -gt $acopy ]]; then
        eval $__resultvar=$BCOPY
    else
        eval $__resultvar=$ACOPY
    fi
}

# start lproxy
start() {
    echo 'start lproxy-cv'
    ulimit -n 100000
    get_copy_run path
    echo "run with copy:$path"
    $path -u $UUID 2>&1 | logger &
    #$path
}

# stop lproxy
stop() {
    echo 'stop lproxy-cv'
    if [[ -e "$PIDFILE" ]]; then
        local pid=$(cat $PIDFILE)
        if [[ -n "$pid" ]]; then
            kill -s USR1 $pid > /dev/null 2>&1
            if [[ $? -ne 0 ]]
            then
                return
            fi
            local count=0
            while [[ -e "/proc/$pid" ]]
            do
                sleep 1
                count=$((count+1))
                # test if it has exited
                if [[ $count -gt 5 ]]; then
                    count=0
                    # force exit
                    kill -9 $pid > /dev/null 2>&1
                    if [[ $? -ne 0 ]]
                    then
                        return
                    fi
                fi
            done
        fi
    fi
}

# restart
restart() {
    echo 'restart lproxy-cv'
    stop
    start
}

# period monitor
monitor() {
    echo 'monitor lproxy-cv'
    local needcreate=1
    if [[ -e $PIDFILE ]]; then
        local pid=$(cat $PIDFILE)
        if [[ -n $pid ]]; then
            if [[ -e /proc/$pid ]]; then
                needcreate=0
            fi
        fi
    fi

    if [[ $needcreate -ne 0 ]]; then
        start
    else
        echo "lproxy-cv already started"
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

