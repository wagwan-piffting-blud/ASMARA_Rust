<?php

function resolve_id($id) {
    $files = glob(getenv("RECORDING_DIR") . "/EAS_Recording_*.wav");

    usort($files, function($a, $b) {
        return filemtime($a) - filemtime($b);
    });

    return $files[$id];
}

function hhmmToSeconds(string $hhmmString): int {
    if (strlen($hhmmString) !== 4 || !ctype_digit($hhmmString)) {
        throw new InvalidArgumentException("Input must be a 4-digit numeric string representing HHMM.");
    }

    $hours = (int) substr($hhmmString, 0, 2);
    $minutes = (int) substr($hhmmString, 2, 2);

    if ($hours < 0 || $minutes < 0 || $minutes >= 60) {
        throw new InvalidArgumentException("Invalid HHMM format. Hours or minutes are out of range.");
    }

    $totalSeconds = ($hours * 3600) + ($minutes * 60);

    return $totalSeconds;
}

if(!session_id()) {
    if(getenv('USE_REVERSE_PROXY') === 'true') {
        session_set_cookie_params(259200, "/", "", true, true);
    }

    else {
        session_set_cookie_params(259200, "/", "", false, true);
    }
    session_start();
}

$requestHeaders = getallheaders();

if(isset($requestHeaders['Authorization']) && $requestHeaders['Authorization'] === "Bearer " . base64_encode(getenv('DASHBOARD_USERNAME') . ':' . getenv('DASHBOARD_PASSWORD'))) {
    $_SESSION['authed'] = true;
}

if(!isset($_SESSION['authed'])) {
    header("Location: index.php?redirect=" . urlencode($_SERVER['REQUEST_URI']));
    exit();
}

if($_GET["recording_id"] !== null && isset($_GET["recording_id"]) && $_SESSION['authed'] === true) {
    $file = resolve_id($_GET["recording_id"]);
    if(file_exists($file)) {
        header("Content-Type: audio/wav");
        header('Content-Disposition: attachment; filename="' . basename($file) . '"');
        header('Content-Transfer-Encoding: binary');
        header("Content-Length: " . filesize($file));
        readfile($file);
        exit();
    } else {
        http_response_code(404);
        echo "File not found.";
        exit();
    }
}

if($_GET["latest_id"] !== null && isset($_GET["latest_id"]) && $_SESSION['authed'] === true) {
    $files = glob(getenv("RECORDING_DIR") . "/EAS_Recording_*.wav");

    usort($files, function($a, $b) {
        return filemtime($a) - filemtime($b);
    });

    echo count($files) - 1;
    exit();
}

if(!empty($_GET['fetch_alerts']) && $_SESSION['authed'] === true) {
    date_default_timezone_set(getenv("TZ") ?: "UTC");
    header("Content-Type: application/json");

    $alertsraw = file_get_contents(getenv("SHARED_STATE_DIR") . "/" . getenv("DEDICATED_ALERT_LOG_FILE"));
    $alerts = explode("\n", trim($alertsraw));
    $alertdata = [];
    $alert_processed = [];
    $idx_offset = 0;

    foreach($alerts as $idx => $alert) {
        if(empty($alert)) {
            $idx_offset += 1;
            continue;
        }

        preg_match('/:(\d{2}) [AP]M\)/', $alert, $seconds);

        $received_at = preg_match('/\(Received @ (.*?)\)$/', $alert, $matches) ? strtotime($matches[1]) : null;
        $length = preg_match('/\+(\d{4})-/', $alert, $matches) ? $matches[1] : null;
        $length_as_secs = hhmmToSeconds($length);
        $expired_at = $received_at + $length_as_secs;

        $alert_severity_raw = preg_match('/has issued a (.*?) for/', $alert, $matches) ? explode(" for ", $matches[1])[0] : null;
        $alert_severity_words_array = preg_split('/(?=[A-Z])/', $alert_severity_raw, -1, PREG_SPLIT_NO_EMPTY);

        if($alert_severity_words_array[2]) {
            $alert_severity = strtolower($alert_severity_words_array[2]);
        }

        else {
            $alert_severity = strtolower($alert_severity_words_array[1]);
        }

        $alert_processed = [
            "received_at" => $received_at,
            "expired_at" => $expired_at,
            "data" => [
                "event_code" => preg_match('/ZCZC-[A-Z]{3}-([A-Z]{3})-/', $alert, $matches) ? $matches[1] : null,
                "event_text" => preg_match('/has issued a (.*?) for/', $alert, $matches) ? explode(" for ", $matches[1])[0] : null,
                "originator" => preg_match('/Message from (.*?)[.;]/', $alert, $matches) ? $matches[1] : null,
                "locations" => preg_match('/for (.*?); beginning/', $alert, $matches) ? $matches[1] : null,
                "alert_severity" => $alert_severity,
                "length" => $length,
                "eas_text" => preg_match('/-: (.*\.) \(/', $alert, $matches) ? $matches[1] : null,
                "audio_recording" => "archive.php?recording_id=" . ($idx - $idx_offset),
            ]
        ];

        $alertdata[] = $alert_processed;
    }

    echo json_encode($alertdata);
    exit();
}

else { ?><!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>EAS Archived Alerts</title>
        <link rel="stylesheet" href="style.css" />
    </head>
    <body>
        <header>
            <h1><img src="assets/favicon-96x96.png" alt="EAS Logo" class="logo" />EAS Archived Alerts</h1>
            <div id="header-right">
                <a class="custom-button" href="index.php" class="button">Back to Dashboard</a>
                <a class="custom-button" href="logout.php" class="button">Logout</a>
            </div>
        </header>
        <main id="oldAlerts">
            <section id="oldAlertSection">
                <h2>
                    Archived/Old Alerts
                    <span id="filterStatus" class="pill">Showing All</span>
                    <span id="filterOptions" class="pill">
                        Filter by...
                    </span>
                    <span id="oldAlertCount" class="pill">None</span>
                </h2>
                <div id="oldAlertList" class="section-scroll"></div>
            </section>
        </main>
    </body>
    <script src="archive.js"></script>
</html>
<?php } ?>
