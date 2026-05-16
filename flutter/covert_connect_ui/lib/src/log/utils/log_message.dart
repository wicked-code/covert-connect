import 'dart:developer';

enum LogLevel {
  // ignore: constant_identifier_names
  INFO,
  // ignore: constant_identifier_names
  WARN,
  // ignore: constant_identifier_names
  ERROR,
}

class LogMessageDto {
  LogMessageDto({required this.timestamp, required this.level, required this.message, required this.target});

  final DateTime timestamp;
  final LogLevel level;
  final String message;
  final String? target;

  static LogMessageDto fromJson(Map<String, dynamic> json) {
    LogLevel level = LogLevel.INFO;
    try {
      level = LogLevel.values.byName(json["level"] as String);
    } catch (e) {
      log("Failed to parse log message JSON: $e");
    }

    String message = "";
    if (json["fields"]["message"] is String) {
      message = json["fields"]["message"] as String;
    } else if (json["fields"] is Map<String, dynamic>) {
      message = (json["fields"] as Map<String, dynamic>).entries.map((e) => "${e.key}: ${e.value}").join(", ");
    }

    return LogMessageDto(
      timestamp: DateTime.parse(json["timestamp"] as String),
      level: level,
      message: message,
      target: json["target"] as String?,
    );
  }
}
