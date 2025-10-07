<?php

if(session_id() !== null) {
    session_destroy();
    $_COOKIE["PHPSESSID"] = "";
    setcookie("PHPSESSID", "", time() - 3600, "/");
    echo "<script>alert('Logged out.'); window.location='/index.php';</script>";
    exit();
}
