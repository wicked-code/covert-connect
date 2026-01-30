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
    return LogMessageDto(
      timestamp: DateTime.parse(json["timestamp"] as String),
      level: LogLevel.values.byName(json["level"] as String),
      message: json["fields"]["message"] as String,
      target: json["target"] as String?,
    );
  }
}
