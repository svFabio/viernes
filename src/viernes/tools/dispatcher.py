import time
from typing import Any

from pydantic import ValidationError

from viernes.tools.audit import AuditLogger
from viernes.tools.confirmation import ConfirmationManager
from viernes.tools.registry import ToolRegistry
from viernes.tools.security import SecurityViolationError, inspect_for_shell_injection


class ToolDispatcher:
    """Secure dispatcher for executing authorized tools with strict validation and auditing."""

    def __init__(
        self,
        registry: ToolRegistry | None = None,
        audit_logger: AuditLogger | None = None,
        confirmation_manager: ConfirmationManager | None = None,
    ) -> None:
        self.registry = registry or ToolRegistry()
        self.audit_logger = audit_logger or AuditLogger()
        self.confirmation_manager = confirmation_manager or ConfirmationManager()

    def dispatch(
        self,
        tool_name: str,
        raw_params: dict[str, Any] | None = None,
        is_confirmed: bool = False,
    ) -> str:
        start_time = time.perf_counter()
        params = raw_params or {}

        # 1. Allowlist verification
        tool = self.registry.get_tool(tool_name)
        if not tool or not self.registry.is_allowed(tool_name):
            duration = (time.perf_counter() - start_time) * 1000
            err_msg = f"Tool '{tool_name}' is not permitted by the strict security allowlist."
            self.audit_logger.log(
                tool_name=tool_name,
                status="REJECTED",
                duration_ms=duration,
                params=params,
                error_message=err_msg,
            )
            raise SecurityViolationError(err_msg)

        # 2. Shell and injection scanning
        try:
            inspect_for_shell_injection(params)
        except SecurityViolationError as sec_err:
            duration = (time.perf_counter() - start_time) * 1000
            self.audit_logger.log(
                tool_name=tool_name,
                status="REJECTED",
                duration_ms=duration,
                params=params,
                error_message=str(sec_err),
            )
            raise

        # 3. Pydantic schema validation
        try:
            validated_params = tool.param_schema.model_validate(params)
        except ValidationError as val_err:
            duration = (time.perf_counter() - start_time) * 1000
            err_msg = f"Parameter validation error for '{tool_name}': {val_err}"
            self.audit_logger.log(
                tool_name=tool_name,
                status="REJECTED",
                duration_ms=duration,
                params=params,
                error_message=err_msg,
            )
            raise ValueError(err_msg) from val_err

        # 4. Critical destructive action confirmation gate
        if tool.requires_confirmation and not is_confirmed:
            impact = self.registry.format_impact(tool_name, validated_params)
            action = self.confirmation_manager.prepare_request(tool_name, validated_params, impact)
            duration = (time.perf_counter() - start_time) * 1000
            self.audit_logger.log(
                tool_name=tool_name,
                status="PENDING_CONFIRMATION",
                duration_ms=duration,
                params=params,
                error_message=action.spoken_question,
            )
            return action.spoken_question

        # 5. Handler execution
        try:
            raw_result = tool.handler(validated_params)
            duration = (time.perf_counter() - start_time) * 1000
            self.audit_logger.log(
                tool_name=tool_name,
                status="SUCCESS",
                duration_ms=duration,
                params=params,
            )
            return self._format_result(raw_result)
        except Exception as exc:
            duration = (time.perf_counter() - start_time) * 1000
            err_msg = f"Execution error in '{tool_name}': {exc}"
            self.audit_logger.log(
                tool_name=tool_name,
                status="ERROR",
                duration_ms=duration,
                params=params,
                error_message=err_msg,
            )
            raise

    def _format_result(self, result: Any) -> str:
        if isinstance(result, str):
            return result.strip()
        if isinstance(result, (dict, list)):
            import json

            return json.dumps(result, ensure_ascii=False)
        return str(result)
