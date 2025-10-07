# EAS Icons

This repository contains a small collection of Font Awesome icons representing various Emergency Alert System (EAS) messages. These icons are designed to visually convey the severity and type of alerts issued by emergency management agencies. The index.php file is used to serve the icons based on the alert type, and optionally color the icon dependent on alert type. Any color can be used, but the following colors are recommended for consistency with standard EAS practices:

- **Red**: For "Alert" and "Warning" messages, indicating high severity.
- **Orange**: For "Advisory" and "Watch" messages, indicating moderate severity.
- **Yellow**: For "Statement" messages, indicating low severity.
- **Green**: For "Test" messages, indicating non-emergency tests.

This folder is covered under the parent folder's LICENSE file (GNU GPL v3). The public endpoint for the icons is hosted [here](https://wagspuzzle.space/assets/eas-icons/index.php). Feel free to use this endpoint in your applications, but please be considerate when doing so.

# Params

The `index.php` file accepts the following GET parameters:
| Parameter | Description | Example |
|-----------|-------------|---------|
| `code`    | The EAS event code. This is a required parameter. | `?code=RWT` |
| `hex`   | The hex color code for the icon (prefixed "0x"). This is a required parameter. | `&hex=0xFF0000` |

Some example URLs:
- Red - Warning: `https://wagspuzzle.space/assets/eas-icons/index.php?code=TOR&hex=0xFF0000`
- Orange - Watch: `https://wagspuzzle.space/assets/eas-icons/index.php?code=SVR&hex=0xFFA500`
- Yellow - Statement: `https://wagspuzzle.space/assets/eas-icons/index.php?code=SVA&hex=0xFFFF00`
- Green - Test: `https://wagspuzzle.space/assets/eas-icons/index.php?code=RWT&hex=0x105733`

# Credits

Icons are sourced from [Font Awesome](https://fontawesome.com/) and are used under their [Free License](https://fontawesome.com/license/free). The icons were created by [Dave Gandy](https://fontawesome.com/). The PHP script was developed by myself with minor help from Gemini AI by Google for PNG conversion.
